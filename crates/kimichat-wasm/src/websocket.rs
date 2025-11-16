use gloo_net::websocket::{futures::WebSocket, Message as WsMessage};
use futures::{StreamExt, SinkExt};
use wasm_bindgen::JsValue;
use crate::protocol::{ClientMessage, ServerMessage};

/// WebSocket client wrapper
pub struct WsClient {
    ws: WebSocket,
}

impl WsClient {
    /// Create a new WebSocket client
    pub async fn connect(url: &str) -> Result<Self, JsValue> {
        log::info!("Connecting to WebSocket: {}", url);
        let ws = WebSocket::open(url)
            .map_err(|e| JsValue::from_str(&format!("Failed to connect: {:?}", e)))?;

        Ok(Self { ws })
    }

    /// Send a message to the server
    pub async fn send(&mut self, msg: ClientMessage) -> Result<(), JsValue> {
        let json = serde_json::to_string(&msg)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize: {}", e)))?;

        log::debug!("Sending message: {}", json);

        self.ws.send(WsMessage::Text(json)).await
            .map_err(|e| JsValue::from_str(&format!("Failed to send: {:?}", e)))?;

        Ok(())
    }

    /// Receive the next message from the server
    pub async fn receive(&mut self) -> Result<Option<ServerMessage>, JsValue> {
        match self.ws.next().await {
            Some(Ok(WsMessage::Text(text))) => {
                log::debug!("Received message: {}", text);
                let msg = serde_json::from_str(&text)
                    .map_err(|e| JsValue::from_str(&format!("Failed to parse: {}", e)))?;
                Ok(Some(msg))
            }
            Some(Ok(WsMessage::Bytes(_))) => {
                log::warn!("Received unexpected binary message");
                Ok(None)
            }
            Some(Err(e)) => {
                Err(JsValue::from_str(&format!("WebSocket error: {:?}", e)))
            }
            None => {
                log::info!("WebSocket connection closed");
                Ok(None)
            }
        }
    }

    /// Close the WebSocket connection
    pub async fn close(self) -> Result<(), JsValue> {
        self.ws.close(None, None)
            .map_err(|e| JsValue::from_str(&format!("Failed to close: {:?}", e)))?;
        Ok(())
    }
}

/// Split WebSocket into sender and receiver
pub fn split_websocket(ws: WebSocket) -> (WsSender, WsReceiver) {
    let (sink, stream) = ws.split();
    (WsSender { sink }, WsReceiver { stream })
}

/// WebSocket sender
pub struct WsSender {
    sink: futures::stream::SplitSink<WebSocket, WsMessage>,
}

impl WsSender {
    pub async fn send(&mut self, msg: ClientMessage) -> Result<(), JsValue> {
        let json = serde_json::to_string(&msg)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize: {}", e)))?;

        log::debug!("Sending message: {}", json);

        self.sink.send(WsMessage::Text(json)).await
            .map_err(|e| JsValue::from_str(&format!("Failed to send: {:?}", e)))?;

        Ok(())
    }
}

/// WebSocket receiver
pub struct WsReceiver {
    stream: futures::stream::SplitStream<WebSocket>,
}

impl WsReceiver {
    pub async fn receive(&mut self) -> Result<Option<ServerMessage>, JsValue> {
        match self.stream.next().await {
            Some(Ok(WsMessage::Text(text))) => {
                log::debug!("Received message: {}", text);
                let msg = serde_json::from_str(&text)
                    .map_err(|e| JsValue::from_str(&format!("Failed to parse: {}", e)))?;
                Ok(Some(msg))
            }
            Some(Ok(WsMessage::Bytes(_))) => {
                log::warn!("Received unexpected binary message");
                Ok(None)
            }
            Some(Err(e)) => {
                Err(JsValue::from_str(&format!("WebSocket error: {:?}", e)))
            }
            None => {
                log::info!("WebSocket connection closed");
                Ok(None)
            }
        }
    }
}
