use tauri::ipc::Channel;
use tokio::sync::mpsc::UnboundedSender;

pub trait MessageSender<T>: Send + Sync {
    fn send(&self, msg: T) -> Result<(), String>;
}

impl<T: Send + Sync + 'static + serde::Serialize> MessageSender<T> for Channel<T> {
    fn send(&self, msg: T) -> Result<(), String> {
        self.send(msg).map_err(|e| e.to_string())
    }
}

impl<T: Send + Sync + 'static> MessageSender<T> for UnboundedSender<T> {
    fn send(&self, msg: T) -> Result<(), String> {
        self.send(msg).map_err(|e| e.to_string())
    }
}
