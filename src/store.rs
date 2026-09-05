use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};

const STATE_FILE_NAME: &str = "state.toml";
const TEMPORARY_STATE_FILE_NAME: &str = "state.toml.tmp";
const TEMPORARY_WAV_FILE_NAME: &str = "audio.wav.tmp";

#[derive(Deserialize, Serialize)]
pub(super) struct TargetState {
    pub(super) input_hash: String,
    pub(super) speaker: u32,
    pub(super) wav_hash: String,
}

pub(super) struct Store {
    wav_dir: PathBuf,
}

impl Store {
    pub(super) fn open(data_dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let wav_dir = data_dir.as_ref().join("audio");

        fs::create_dir_all(&wav_dir)
            .with_context(|| format!("failed to create WAV directory: {}", wav_dir.display()))?;

        Ok(Self { wav_dir })
    }

    pub(super) fn load_state(&self, id: &str) -> anyhow::Result<Option<TargetState>> {
        let state_path = self.target_dir(id).join(STATE_FILE_NAME);
        let source = match fs::read_to_string(&state_path) {
            Ok(source) => source,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to read state: {}", state_path.display()));
            }
        };

        let state: TargetState = toml::from_str(&source)
            .with_context(|| format!("failed to parse state: {}", state_path.display()))?;

        Ok(Some(state))
    }

    pub(super) fn save_state(&self, id: &str, state: &TargetState) -> anyhow::Result<()> {
        let target_dir = self.target_dir(id);
        fs::create_dir_all(&target_dir).with_context(|| {
            format!(
                "failed to create target directory: {}",
                target_dir.display()
            )
        })?;

        let source = toml::to_string(state).context("failed to serialize state")?;
        let temporary_path = target_dir.join(TEMPORARY_STATE_FILE_NAME);
        let state_path = target_dir.join(STATE_FILE_NAME);

        fs::write(&temporary_path, source).with_context(|| {
            format!(
                "failed to write temporary state: {}",
                temporary_path.display()
            )
        })?;

        replace_file(&temporary_path, &state_path)
    }

    pub(super) fn wav_path(&self, id: &str, wav_hash: &str) -> PathBuf {
        self.target_dir(id).join(format!("{wav_hash}.wav"))
    }

    pub(super) fn temporary_wav_path(&self, id: &str) -> anyhow::Result<PathBuf> {
        let target_dir = self.target_dir(id);
        fs::create_dir_all(&target_dir).with_context(|| {
            format!(
                "failed to create target directory: {}",
                target_dir.display()
            )
        })?;

        Ok(target_dir.join(TEMPORARY_WAV_FILE_NAME))
    }

    pub(super) fn commit_wav(&self, id: &str, wav_hash: &str) -> anyhow::Result<()> {
        let temporary_path = self.target_dir(id).join(TEMPORARY_WAV_FILE_NAME);
        let wav_path = self.wav_path(id, wav_hash);

        if wav_path.is_file() {
            fs::remove_file(&temporary_path).with_context(|| {
                format!(
                    "failed to remove temporary WAV file: {}",
                    temporary_path.display()
                )
            })?;
        } else {
            fs::rename(&temporary_path, &wav_path)
                .with_context(|| format!("failed to save WAV file: {}", wav_path.display()))?;
        }

        Ok(())
    }

    pub(super) fn remove_wav(&self, id: &str, wav_hash: &str) -> anyhow::Result<()> {
        let wav_path = self.wav_path(id, wav_hash);

        match fs::remove_file(&wav_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error)
                .with_context(|| format!("failed to remove WAV file: {}", wav_path.display())),
        }
    }

    fn target_dir(&self, id: &str) -> PathBuf {
        self.wav_dir.join(id)
    }
}

fn replace_file(source: &Path, destination: &Path) -> anyhow::Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) if destination.exists() => {
            fs::remove_file(destination).with_context(|| {
                format!("failed to remove existing file: {}", destination.display())
            })?;

            fs::rename(source, destination)
                .with_context(|| format!("failed to replace file: {}", destination.display()))
        }
        Err(error) => {
            Err(error).with_context(|| format!("failed to replace file: {}", destination.display()))
        }
    }
}
