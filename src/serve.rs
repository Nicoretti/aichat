use crate::{
    client::{
        init_client, ClientConfig, CompletionDetails, Message, Model, SendData, SseEvent,
        SseHandler,
    },
    config::{Config, GlobalConfig},
    utils::create_abort_signal,
};

use anyhow::{anyhow, bail, Result};
use bytes::Bytes;
use chrono::{Timelike, Utc};
use futures_util::StreamExt;
use http::{Method, Response, StatusCode};
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::{
    body::{Frame, Incoming},
    service::service_fn,
};
use hyper_util::rt::{TokioExecutor, TokioIo};
use parking_lot::RwLock;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{convert::Infallible, net::IpAddr, sync::Arc};
use tokio::{
    net::TcpListener,
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
};
use tokio_graceful::Shutdown;
use tokio_stream::wrappers::UnboundedReceiverStream;

const DEFAULT_ADDRESS: &str = "127.0.0.1:8000";
const DEFAULT_MODEL_NAME: &str = "default";

type AppResponse = Response<BoxBody<Bytes, Infallible>>;

pub async fn run(config: GlobalConfig, addr: Option<String>) -> Result<()> {
    let addr = match addr {
        Some(addr) => {
            if let Ok(port) = addr.parse::<u16>() {
                format!("127.0.0.1:{port}")
            } else if let Ok(ip) = addr.parse::<IpAddr>() {
                format!("{ip}:8000")
            } else {
                addr
            }
        }
        None => DEFAULT_ADDRESS.to_string(),
    };
    let clients = config.read().clients.clone();
    let model = config.read().model.clone();
    let listener = TcpListener::bind(&addr).await?;
    let server = Arc::new(Server { clients, model });
    let stop_server = server.run(listener).await?;
    println!("Access the chat completion API at: http://{addr}/v1/chat/completions");
    shutdown_signal().await;
    let _ = stop_server.send(());
    Ok(())
}

struct Server {
    clients: Vec<ClientConfig>,
    model: Model,
}

impl Server {
    async fn run(self: Arc<Self>, listener: TcpListener) -> Result<oneshot::Sender<()>> {
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            let shutdown = Shutdown::new(async { rx.await.unwrap_or_default() });
            let guard = shutdown.guard_weak();

            loop {
                tokio::select! {
                    res = listener.accept() => {
                        let Ok((cnx, _)) = res else {
                            continue;
                        };

                        let stream = TokioIo::new(cnx);
                        let server = self.clone();
                        shutdown.spawn_task(async move {
                            let hyper_service = service_fn(move |request: hyper::Request<Incoming>| {
                                server.clone().handle(request)
                            });
                            let _ = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                                .serve_connection_with_upgrades(stream, hyper_service)
                                .await;
                        });
                    }
                    _ = guard.cancelled() => {
                        break;
                    }
                }
            }
        });
        Ok(tx)
    }

    async fn handle(
        self: Arc<Self>,
        req: hyper::Request<Incoming>,
    ) -> std::result::Result<AppResponse, hyper::Error> {
        let method = req.method().clone();
        let uri = req.uri().clone();
        let mut status = StatusCode::OK;
        let res = if method == Method::POST && uri == "/v1/chat/completions" {
            self.chat_completion(req).await
        } else if method == Method::OPTIONS && uri == "/v1/chat/completions" {
            status = StatusCode::NO_CONTENT;
            Ok(Response::default())
        } else {
            status = StatusCode::NOT_FOUND;
            Err(anyhow!("The requested endpoint was not found."))
        };
        let mut res = match res {
            Ok(res) => {
                info!("{method} {uri} {}", status.as_u16());
                res
            }
            Err(err) => {
                status = StatusCode::BAD_REQUEST;
                error!("{method} {uri} {} {err}", status.as_u16());
                ret_err(err)
            }
        };
        *res.status_mut() = status;
        set_cors_header(&mut res);
        Ok(res)
    }

    async fn chat_completion(&self, req: hyper::Request<Incoming>) -> Result<AppResponse> {
        let req_body = req.collect().await?.to_bytes();
        let req_body: ChatCompletionReqBody = serde_json::from_slice(&req_body)
            .map_err(|err| anyhow!("Invalid request body, {err}"))?;

        let ChatCompletionReqBody {
            model,
            messages,
            temperature,
            top_p,
            max_tokens,
            stream,
        } = req_body;

        let config = Config {
            clients: self.clients.to_vec(),
            model: self.model.clone(),
            ..Default::default()
        };
        let config = Arc::new(RwLock::new(config));

        let (model_name, change) = if model == DEFAULT_MODEL_NAME {
            (self.model.id(), true)
        } else if self.model.id() == model {
            (model, false)
        } else {
            (model, true)
        };

        if change {
            config.write().set_model(&model_name)?;
        }

        let mut client = init_client(&config)?;
        if max_tokens.is_some() {
            client.model_mut().set_max_output_tokens(max_tokens);
        }
        let abort = create_abort_signal();
        let http_client = client.build_client()?;

        let completion_id = generate_completion_id();
        let created = Utc::now().timestamp();

        let send_data: SendData = SendData {
            messages,
            temperature,
            top_p,
            stream,
        };

        if stream {
            let (tx, mut rx) = unbounded_channel();
            tokio::spawn(async move {
                let mut is_first = true;
                let (tx2, rx2) = unbounded_channel();
                let mut handler = SseHandler::new(tx2, abort);
                async fn map_event(
                    mut rx: UnboundedReceiver<SseEvent>,
                    tx: &UnboundedSender<ResEvent>,
                    is_first: &mut bool,
                ) {
                    while let Some(reply_event) = rx.recv().await {
                        if *is_first {
                            let _ = tx.send(ResEvent::First(None));
                            *is_first = false;
                        }
                        match reply_event {
                            SseEvent::Text(text) => {
                                let _ = tx.send(ResEvent::Text(text));
                            }
                            SseEvent::Done => {
                                let _ = tx.send(ResEvent::Done);
                            }
                        }
                    }
                }
                tokio::select! {
                    _ = map_event(rx2, &tx, &mut is_first) => {}
                    ret = client.send_message_streaming_inner(&http_client, &mut handler, send_data) => {
                        if let Err(err) = ret {
                            send_first_event(&tx, Some(format!("{err:?}")), &mut is_first)
                        }
                        let _ = tx.send(ResEvent::Done);
                    }
                }
            });

            let first_event = rx.recv().await;

            if let Some(ResEvent::First(Some(err))) = first_event {
                bail!("{err}");
            }

            let shared: Arc<(String, String, i64)> = Arc::new((completion_id, model_name, created));
            let stream = UnboundedReceiverStream::new(rx);
            let stream = stream.filter_map(move |res_event| {
                let shared = shared.clone();
                async move {
                    let (completion_id, model, created) = shared.as_ref();
                    match res_event {
                        ResEvent::Text(text) => Some(Ok(create_frame(
                            completion_id,
                            model,
                            *created,
                            &text,
                            false,
                        ))),
                        ResEvent::Done => {
                            Some(Ok(create_frame(completion_id, model, *created, "", true)))
                        }
                        _ => None,
                    }
                }
            });
            let res = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/event-stream")
                .header("Cache-Control", "no-cache")
                .header("Connection", "keep-alive")
                .body(BodyExt::boxed(StreamBody::new(stream)))?;
            Ok(res)
        } else {
            let (content, details) = client.send_message_inner(&http_client, send_data).await?;
            let res = Response::builder()
                .header("Content-Type", "application/json")
                .body(
                    Full::new(ret_non_stream(
                        &completion_id,
                        &model_name,
                        created,
                        &content,
                        &details,
                    ))
                    .boxed(),
                )?;
            Ok(res)
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionReqBody {
    model: String,
    messages: Vec<Message>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_tokens: Option<isize>,
    #[serde(default)]
    stream: bool,
}

#[derive(Debug)]
enum ResEvent {
    First(Option<String>),
    Text(String),
    Done,
}

fn send_first_event(tx: &UnboundedSender<ResEvent>, data: Option<String>, is_first: &mut bool) {
    if *is_first {
        let _ = tx.send(ResEvent::First(data));
        *is_first = false;
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler")
}

fn generate_completion_id() -> String {
    let random_id = chrono::Utc::now().nanosecond();
    format!("chatcmpl-{}", random_id)
}

fn set_cors_header(res: &mut AppResponse) {
    res.headers_mut().insert(
        hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN,
        hyper::header::HeaderValue::from_static("*"),
    );
    res.headers_mut().insert(
        hyper::header::ACCESS_CONTROL_ALLOW_METHODS,
        hyper::header::HeaderValue::from_static("GET,POST,PUT,PATCH,DELETE"),
    );
    res.headers_mut().insert(
        hyper::header::ACCESS_CONTROL_ALLOW_HEADERS,
        hyper::header::HeaderValue::from_static("Content-Type,Authorization"),
    );
}

fn create_frame(id: &str, model: &str, created: i64, content: &str, done: bool) -> Frame<Bytes> {
    let (delta, finish_reason) = if done {
        (json!({}), "stop".into())
    } else {
        let delta = if content.is_empty() {
            json!({ "role": "assistant", "content": content })
        } else {
            json!({ "content": content })
        };
        (delta, Value::Null)
    };
    let value = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [
            {
                "index": 0,
                "delta": delta,
                "finish_reason": finish_reason,
            },
        ],
    });
    let output = if done {
        format!("data: {value}\n\ndata: [DONE]\n\n")
    } else {
        format!("data: {value}\n\n")
    };
    Frame::data(Bytes::from(output))
}

fn ret_non_stream(
    id: &str,
    model: &str,
    created: i64,
    content: &str,
    details: &CompletionDetails,
) -> Bytes {
    let id = details.id.as_deref().unwrap_or(id);
    let input_tokens = details.input_tokens.unwrap_or_default();
    let output_tokens = details.output_tokens.unwrap_or_default();
    let total_tokens = input_tokens + output_tokens;
    let res_body = json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [
            {
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": content,
                },
                "logprobs": null,
                "finish_reason": "stop",
            },
        ],
        "usage": {
            "prompt_tokens": input_tokens,
            "completion_tokens": output_tokens,
            "total_tokens": total_tokens,
        },
    });
    Bytes::from(res_body.to_string())
}

fn ret_err<T: std::fmt::Display>(err: T) -> AppResponse {
    let data = json!({
        "error": {
            "message": err.to_string(),
            "type": "invalid_request_error",
        },
    });
    Response::builder()
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(data.to_string())).boxed())
        .unwrap()
}
