use std::{io::ErrorKind, path::Path, sync::Arc};

use actix_files::{Files, NamedFile};
use actix_web::{App, HttpResponse, HttpServer, Responder, error, web};
use serde::Deserialize;
use tracing_actix_web::TracingLogger;

use crate::{config::Config, progress::Progress, store::Store};

struct ServerState {
    config: Arc<Config>,
    store: Arc<Store>,
    progress: Arc<Progress>,
}

#[derive(askama::Template, askama_web::WebTemplate)]
#[template(path = "index.html")]
struct IndexTemplate {
    targets: Vec<String>,
}

async fn index(state: web::Data<ServerState>) -> impl Responder {
    IndexTemplate {
        targets: state
            .config
            .targets
            .iter()
            .map(|target| target.id.clone())
            .collect(),
    }
}

#[derive(askama::Template, askama_web::WebTemplate)]
#[template(path = "player.html")]
struct PlayerTemplate {
    id: String,
    wav_hash: Option<String>,
}

#[derive(Deserialize)]
struct AudioQuery {
    #[serde(rename = "hash")]
    wav_hash: Option<String>,
}

async fn player(
    id: web::Path<String>,
    state: web::Data<ServerState>,
) -> actix_web::Result<PlayerTemplate> {
    let id = id.into_inner();
    if !state.config.targets.iter().any(|target| target.id == id) {
        return Err(error::ErrorNotFound("target not found"));
    }
    let target_state = state.store.load_state(&id).map_err(|error| {
        tracing::error!(
            target = %id,
            error = %format_args!("{error:#}"),
            "failed to load target state"
        );
        error::ErrorInternalServerError("failed to load target state")
    })?;

    Ok(PlayerTemplate {
        wav_hash: target_state
            .filter(|target| state.store.wav_path(&id, &target.wav_hash).is_file())
            .map(|target| target.wav_hash),
        id,
    })
}

async fn audio(
    id: web::Path<String>,
    query: web::Query<AudioQuery>,
    state: web::Data<ServerState>,
) -> actix_web::Result<NamedFile> {
    let id = id.into_inner();
    let wav_hash = match &query.wav_hash {
        Some(wav_hash) => wav_hash.clone(),
        None => {
            state
                .store
                .load_state(&id)
                .map_err(|error| {
                    tracing::error!(
                        target = %id,
                        error = %format_args!("{error:#}"),
                        "failed to load target state"
                    );
                    error::ErrorInternalServerError("failed to load target state")
                })?
                .ok_or_else(|| error::ErrorNotFound("audio not found"))?
                .wav_hash
        }
    };

    if Path::new(&wav_hash).file_name() != Some(wav_hash.as_ref()) {
        return Err(error::ErrorNotFound("audio not found"));
    }

    let wav_path = state.store.wav_path(&id, &wav_hash);
    NamedFile::open(&wav_path).map_err(|io_error| {
        if io_error.kind() == ErrorKind::NotFound {
            error::ErrorNotFound("audio not found")
        } else {
            tracing::error!(
                path = %wav_path.display(),
                error = %io_error,
                "failed to open WAV file"
            );
            error::ErrorInternalServerError("failed to open WAV file")
        }
    })
}

async fn progress(
    id: web::Path<String>,
    state: web::Data<ServerState>,
) -> actix_web::Result<HttpResponse> {
    if !state
        .config
        .targets
        .iter()
        .any(|target| target.id == id.as_str())
    {
        return Err(error::ErrorNotFound("target not found"));
    }
    let progress = state.progress.get(&id);
    Ok(HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(progress))
}

fn routes(config: &mut web::ServiceConfig) {
    config
        .route("/", web::get().to(index))
        .route("/play/{id:[A-Za-z0-9_-]+}", web::get().to(player))
        .route("/audio/{id:[A-Za-z0-9_-]+}.wav", web::get().to(audio))
        .route("/progress/{id:[A-Za-z0-9_-]+}", web::get().to(progress))
        .service(Files::new("/static", "./static"));
}

pub(super) async fn run(
    config: Arc<Config>,
    store: Arc<Store>,
    progress: Arc<Progress>,
) -> std::io::Result<()> {
    let bind_addr = config.bind_addr.clone();
    let state = web::Data::new(ServerState {
        config,
        store,
        progress,
    });

    HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .app_data(state.clone())
            .configure(routes)
    })
    .bind(bind_addr)?
    .run()
    .await
}
