use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, ensure};
use serde::Deserialize;

const DEFAULT_CONFIG_PATH: &str = "config.toml";
const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_DATA_DIR: &str = "./data";
const DEFAULT_VOICEVOX_URL: &str = "http://127.0.0.1:50021";
const DEFAULT_POLL_INTERVAL_SECONDS: u64 = 60 * 5;

pub(super) struct Config {
    pub(super) bind_addr: String,
    pub(super) data_dir: PathBuf,
    pub(super) voicevox_url: String,
    pub(super) poll_interval_seconds: u64,
    pub(super) voicevox: VoicevoxConfig,
    pub(super) targets: Vec<TargetConfig>,
}

#[derive(Deserialize)]
pub(super) struct VoicevoxConfig {
    pub(super) speaker: u32,
}

#[derive(Deserialize)]
pub(super) struct TargetConfig {
    pub(super) id: String,
    pub(super) input_path: PathBuf,
}

impl Config {
    pub(super) fn load() -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct FileConfig {
            voicevox: VoicevoxConfig,
            #[serde(rename = "target")]
            targets: Vec<TargetConfig>,
        }

        let config_path = env::var("CONFIG_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_PATH));

        let source = fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read config: {}", config_path.display()))?;

        let mut file: FileConfig = toml::from_str(&source)
            .with_context(|| format!("failed to parse config: {}", config_path.display()))?;

        validate_target_ids(&file.targets)?;
        resolve_input_paths(&mut file.targets, &config_path);

        let poll_interval_seconds: u64 = env::var("POLL_INTERVAL_SECONDS")
            .unwrap_or_else(|_| DEFAULT_POLL_INTERVAL_SECONDS.to_string())
            .parse()
            .context("POLL_INTERVAL_SECONDS must be a positive integer")?;
        ensure!(
            poll_interval_seconds >= 1,
            "POLL_INTERVAL_SECONDS must be a positive integer"
        );
        let voicevox_url = env::var("VOICEVOX_URL")
            .unwrap_or_else(|_| DEFAULT_VOICEVOX_URL.to_string())
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string()),
            data_dir: env::var("DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(DEFAULT_DATA_DIR)),
            voicevox_url,
            poll_interval_seconds,
            voicevox: file.voicevox,
            targets: file.targets,
        })
    }
}

fn validate_target_ids(targets: &[TargetConfig]) -> anyhow::Result<()> {
    let mut ids = HashSet::new();

    for target in targets {
        ensure!(
            !target.id.is_empty()
                && target
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
            "invalid target ID: {}",
            target.id
        );

        ensure!(ids.insert(&target.id), "duplicate target ID: {}", target.id);
    }

    Ok(())
}

fn resolve_input_paths(targets: &mut [TargetConfig], config_path: &Path) {
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

    for target in targets {
        if target.input_path.is_relative() {
            target.input_path = config_dir.join(&target.input_path);
        }
    }
}
