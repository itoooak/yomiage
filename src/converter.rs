use std::{
    fs::{self, File},
    io::Cursor,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, ensure};
use hound::{WavReader, WavSpec, WavWriter};
use reqwest::Client;
use serde::Serialize;

use crate::{
    config::{Config, TargetConfig},
    progress::Progress,
    store::{Store, TargetState},
};

const SAMPLING_RATE_HZ: u32 = 24000;

pub(super) enum ConversionStatus {
    Converted,
    Unchanged,
}

pub(super) struct Converter {
    client: Client,
    voicevox_url: String,
    speaker: u32,
    store: Arc<Store>,
    progress: Arc<Progress>,
}

impl Converter {
    pub(super) fn new(config: &Config, store: Arc<Store>, progress: Arc<Progress>) -> Self {
        Self {
            client: Client::new(),
            voicevox_url: config.voicevox_url.clone(),
            speaker: config.voicevox.speaker,
            store,
            progress,
        }
    }

    #[tracing::instrument(
        name = "conversion",
        skip_all,
        fields(
            target = %target.id,
            input_path = %target.input_path.display(),
            input_hash = tracing::field::Empty,
            wav_hash = tracing::field::Empty,
        )
    )]
    pub(super) async fn convert(&self, target: &TargetConfig) -> anyhow::Result<ConversionStatus> {
        let started_at = Instant::now();
        let result = self.convert_input(target, started_at).await;
        let elapsed_seconds = started_at.elapsed().as_secs_f64();
        match &result {
            Ok(ConversionStatus::Converted) => {
                tracing::info!(elapsed_seconds, "updated WAV");
            }
            Ok(ConversionStatus::Unchanged) => {
                tracing::info!(elapsed_seconds, reason = "unchanged", "skipped WAV update");
            }
            Err(error) => {
                tracing::error!(elapsed_seconds, error = %format_args!("{error:#}"), "failed to update WAV");
            }
        }
        result
    }

    async fn convert_input(
        &self,
        target: &TargetConfig,
        started_at: Instant,
    ) -> anyhow::Result<ConversionStatus> {
        tracing::info!(
            elapsed_seconds = started_at.elapsed().as_secs_f64(),
            "checking input"
        );
        let input = fs::read(&target.input_path).with_context(|| {
            format!("failed to read input file: {}", target.input_path.display())
        })?;
        let input_hash = blake3::hash(&input).to_hex().to_string();
        tracing::Span::current().record("input_hash", input_hash.as_str());

        let previous_state = self.store.load_state(&target.id)?;
        tracing::info!(
            previous_input_hash = previous_state
                .as_ref()
                .map(|state| state.input_hash.as_str()),
            previous_wav_hash = previous_state.as_ref().map(|state| state.wav_hash.as_str()),
            elapsed_seconds = started_at.elapsed().as_secs_f64(),
            "checked input version"
        );

        if previous_state.as_ref().is_some_and(|state| {
            state.input_hash == input_hash
                && state.speaker == self.speaker
                && self.store.wav_path(&target.id, &state.wav_hash).is_file()
        }) {
            let wav_hash = previous_state.unwrap().wav_hash;
            tracing::Span::current().record("wav_hash", wav_hash.as_str());
            return Ok(ConversionStatus::Unchanged);
        }

        let input = std::str::from_utf8(&input).with_context(|| {
            format!(
                "failed to decode input file as UTF-8: {}",
                target.input_path.display()
            )
        })?;
        let temporary_path = self.store.temporary_wav_path(&target.id)?;

        if let Err(error) = self
            .write_wav(&target.id, input, &temporary_path, started_at)
            .await
        {
            let _ = fs::remove_file(&temporary_path);
            return Err(error);
        }

        let wav_hash = hash_file(&temporary_path)?;
        tracing::Span::current().record("wav_hash", wav_hash.as_str());
        self.store.commit_wav(&target.id, &wav_hash)?;
        self.store.save_state(
            &target.id,
            &TargetState {
                input_hash,
                speaker: self.speaker,
                wav_hash: wav_hash.clone(),
            },
        )?;

        if let Some(previous_wav_hash) = previous_state.map(|state| state.wav_hash)
            && previous_wav_hash != wav_hash
            && let Err(error) = self.store.remove_wav(&target.id, &previous_wav_hash)
        {
            tracing::warn!(
                previous_wav_hash,
                elapsed_seconds = started_at.elapsed().as_secs_f64(),
                error = %format_args!("{error:#}"),
                "failed to remove previous WAV file"
            );
        }

        Ok(ConversionStatus::Converted)
    }

    async fn write_wav(
        &self,
        id: &str,
        input: &str,
        output_path: &Path,
        started_at: Instant,
    ) -> anyhow::Result<()> {
        let total_lines = input.lines().count();
        self.progress.synthesizing(id, total_lines);
        tracing::info!(
            total_lines,
            elapsed_seconds = started_at.elapsed().as_secs_f64(),
            "started WAV synthesis"
        );
        let mut last_log = Instant::now();
        let spec = WavSpec {
            channels: 1,
            sample_rate: SAMPLING_RATE_HZ,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = WavWriter::create(output_path, spec)
            .with_context(|| format!("failed to create WAV file: {}", output_path.display()))?;

        for (index, line) in input.lines().enumerate() {
            let line = line.trim();

            if line.is_empty() {
                append_silence(&mut writer, 1.0)?;
            } else {
                let data = self.synthesize(line).await?;
                let reader =
                    WavReader::new(Cursor::new(data)).context("failed to read synthesized WAV")?;
                let source_spec = reader.spec();

                ensure!(
                    source_spec.channels == spec.channels
                        && source_spec.sample_rate == spec.sample_rate
                        && source_spec.bits_per_sample == spec.bits_per_sample
                        && source_spec.sample_format == spec.sample_format,
                    "synthesized WAV has an unexpected format: {source_spec:?}"
                );

                for sample in reader.into_samples::<i16>() {
                    writer
                        .write_sample(sample.context("failed to read synthesized WAV sample")?)?;
                }

                append_silence(&mut writer, 0.5)?;
            }
            self.progress.line_completed(id);
            if last_log.elapsed() >= Duration::from_secs(10) || index + 1 == total_lines {
                tracing::info!(
                    completed_lines = index + 1,
                    total_lines,
                    elapsed_seconds = started_at.elapsed().as_secs_f64(),
                    "WAV synthesis progress"
                );
                last_log = Instant::now();
            }
        }

        self.progress.saving(id);
        tracing::info!(
            elapsed_seconds = started_at.elapsed().as_secs_f64(),
            "saving WAV"
        );
        writer
            .finalize()
            .with_context(|| format!("failed to finalize WAV file: {}", output_path.display()))?;

        Ok(())
    }

    async fn synthesize(&self, text: &str) -> anyhow::Result<Vec<u8>> {
        #[derive(Serialize)]
        struct AudioQueryParams<'a> {
            speaker: u32,
            text: &'a str,
        }

        #[derive(Serialize)]
        struct SynthesisParams {
            speaker: u32,
        }

        let audio_query = self
            .client
            .post(format!("{}/audio_query", self.voicevox_url))
            .query(&AudioQueryParams {
                speaker: self.speaker,
                text,
            })
            .send()
            .await
            .context("failed to send audio query request")?
            .error_for_status()
            .context("failed to create audio query")?
            .bytes()
            .await
            .context("failed to read audio query response")?;

        let speech_data = self
            .client
            .post(format!("{}/synthesis", self.voicevox_url))
            .query(&SynthesisParams {
                speaker: self.speaker,
            })
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(audio_query)
            .send()
            .await
            .context("failed to send synthesis request")?
            .error_for_status()
            .context("failed to synthesize speech")?
            .bytes()
            .await
            .context("failed to read synthesis response")?
            .to_vec();

        Ok(speech_data)
    }
}

fn append_silence<T: std::io::Write + std::io::Seek>(
    writer: &mut WavWriter<T>,
    seconds: f64,
) -> anyhow::Result<()> {
    let sample_count = (SAMPLING_RATE_HZ as f64 * seconds) as u32;
    for _ in 0..sample_count {
        writer.write_sample(0)?;
    }
    Ok(())
}

fn hash_file(path: &Path) -> anyhow::Result<String> {
    let file =
        File::open(path).with_context(|| format!("failed to open file: {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    hasher
        .update_reader(file)
        .with_context(|| format!("failed to read file: {}", path.display()))?;

    Ok(hasher.finalize().to_hex().to_string())
}
