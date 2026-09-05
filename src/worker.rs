use std::{sync::Arc, time::Duration};

use tokio::{task::JoinSet, time::sleep};

use crate::{
    config::Config,
    converter::{ConversionStatus, Converter},
    progress::Progress,
    store::Store,
};

pub(super) async fn run(config: Arc<Config>, store: Arc<Store>, progress: Arc<Progress>) {
    let converter = Arc::new(Converter::new(
        &config,
        Arc::clone(&store),
        Arc::clone(&progress),
    ));
    let interval = Duration::from_secs(config.poll_interval_seconds);
    let mut tasks = JoinSet::new();

    for target_index in 0..config.targets.len() {
        tasks.spawn(run_target(
            Arc::clone(&config),
            target_index,
            Arc::clone(&converter),
            interval,
            Arc::clone(&progress),
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
    progress: Arc<Progress>,
) {
    loop {
        let target = &config.targets[target_index];
        let previous = progress.start(&target.id);

        match converter.convert(target).await {
            Ok(ConversionStatus::Converted) => {
                progress.completed(&target.id);
            }
            Ok(ConversionStatus::Unchanged) => {
                progress.unchanged(&target.id, previous);
            }
            Err(_) => {
                progress.failed(&target.id);
            }
        }

        sleep(interval).await;
    }
}
