//! `GET /api/v1/ws`: retratos (≈5 Hz) e avisos em JSON. O navegador não
//! manda cabeçalho próprio no WebSocket, então o token é a 1ª mensagem;
//! sem ela em poucos segundos, a conexão fecha.

use crate::Shared;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::response::Response;
use std::sync::Arc;
use std::time::Duration;
use swoop_api::Push;

/// Tempo para o cliente mandar o token.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Aceita o upgrade (a guarda de Host/Origin já passou).
pub async fn upgrade(State(s): State<Arc<Shared>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| session(s, socket))
}

/// Uma conexão: autentica, depois empurra tudo até o cliente sair.
async fn session(s: Arc<Shared>, mut socket: WebSocket) {
    if !authenticate(&s, &mut socket).await {
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code: close_code::POLICY,
                reason: "token inválido".into(),
            })))
            .await;
        return;
    }
    let mut sub = s.backend.subscribe();
    if send(&mut socket, &sub.current()).await.is_err() {
        return;
    }
    loop {
        let push = tokio::select! {
            p = sub.next() => match p {
                Some(p) => p,
                None => return,
            },
            // O cliente não manda nada depois do token: qualquer fim = sair.
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                Some(Ok(_)) => continue,
            },
        };
        if send(&mut socket, &push).await.is_err() {
            return;
        }
    }
}

/// Espera a 1ª mensagem e confere o token.
async fn authenticate(s: &Shared, socket: &mut WebSocket) -> bool {
    match tokio::time::timeout(AUTH_TIMEOUT, socket.recv()).await {
        Ok(Some(Ok(Message::Text(t)))) => s.token.matches(t.as_str()),
        _ => false,
    }
}

async fn send(socket: &mut WebSocket, push: &Push) -> Result<(), axum::Error> {
    let json = serde_json::to_string(push).expect("Push sempre serializa");
    socket.send(Message::Text(json.into())).await
}
