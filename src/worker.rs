use std::{sync::Arc, time::Duration};

use tokio::{task::JoinSet, time::sleep};

use crate::{
    config::Config,
    converter::{ConversionStatus, Converter},
    store::Store,
};

pub(super) async fn run(config: Arc<Config>, store: Store) {
    let converter = Arc::new(Converter::new(&config, store));
    let interval = Duration::from_secs(config.poll_interval_seconds);
    let mut tasks = JoinSet::new();

    for target_index in 0..config.targets.len() {
        tasks.spawn(run_target(
            config.clone(),
            target_index,
            converter.clone(),
            interval,
        ));
    }

    while let Some(result) = tasks.join_next().await {
        if let Err(error) = result {
            tracing::error!(%error, "worker task stopped unexpectedly");
        }
    }
}

async fn run_target(
    config: Arc<Config>,
    target_index: usize,
    converter: Arc<Converter>,
    interval: Duration,
) {
    loop {
        let target = &config.targets[target_index];

        match converter.convert(target).await {
            Ok(ConversionStatus::Converted) => {
                tracing::info!(target = %target.id, "updated WAV")
            }
            Ok(ConversionStatus::Unchanged) => {
                tracing::info!(
                    target = %target.id,
                    reason = "unchanged",
                    "skipped WAV update"
                )
            }
            Err(error) => {
                tracing::error!(
                    target = %target.id,
                    error = %format_args!("{error:#}"),
                    "failed to update WAV"
                )
            }
        }

        sleep(interval).await;
    }
}
