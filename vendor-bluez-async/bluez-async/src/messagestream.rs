use dbus::nonblock::{MsgMatch, SyncConnection};
use dbus::Message;
use futures::channel::mpsc::UnboundedReceiver;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// Wrapper for a stream of D-Bus messages which automatically removes the `MsgMatch` from the D-Bus
/// connection when it is dropped.
pub struct MessageStream {
    msg_match: Option<MsgMatch>,
    events: UnboundedReceiver<Message>,
    connection: Arc<SyncConnection>,
}

impl MessageStream {
    pub fn new(msg_match: MsgMatch, connection: Arc<SyncConnection>) -> Self {
        let (msg_match, events) = msg_match.msg_stream();
        Self {
            msg_match: Some(msg_match),
            events,
            connection,
        }
    }
}

impl Stream for MessageStream {
    type Item = Message;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.events).poll_next(cx)
    }
}

impl Drop for MessageStream {
    fn drop(&mut self) {
        let connection = self.connection.clone();
        let msg_match = self.msg_match.take().unwrap();
        tokio::spawn(async move {
            if let Err(e) = connection.remove_match(msg_match.token()).await {
                // The match may already be gone (e.g. the D-Bus connection was reset, or
                // BlueZ already dropped it during teardown/disconnect). This is harmless:
                // the goal of removing it was to free resources that are already gone.
                // See https://github.com/bluez-rs/bluez-async/issues (panic on unwrap here).
                log::debug!("Failed to remove D-Bus match (already removed?): {}", e);
            }
        });
    }
}
