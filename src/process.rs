use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// Tokio tasks detach when a JoinHandle is dropped. Pipe readers must instead
/// stop with their owning operation, including errors and cancellation.
pub struct TaskGuard<T> {
    handle: Option<tokio::task::JoinHandle<T>>,
}
impl<T> TaskGuard<T> {
    pub fn new(handle: tokio::task::JoinHandle<T>) -> Self {
        Self {
            handle: Some(handle),
        }
    }
    pub async fn join(mut self) -> Result<T, tokio::task::JoinError> {
        let result = self.handle.as_mut().unwrap().await;
        self.handle.take();
        result
    }
}
impl<T> Drop for TaskGuard<T> {
    fn drop(&mut self) {
        if let Some(handle) = &self.handle {
            handle.abort();
        }
    }
}
/// Read one UTF-8 line, including its delimiter, without exceeding the byte cap.
pub async fn bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> std::io::Result<Option<String>> {
    let mut bytes = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            break;
        }
        let count = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |n| n + 1);
        if count > limit.saturating_sub(bytes.len()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Child output line exceeded its byte limit",
            ));
        }
        let complete = available[count - 1] == b'\n';
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if complete {
            break;
        }
    }
    if bytes.is_empty() {
        Ok(None)
    } else {
        String::from_utf8(bytes).map(Some).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Child output is not UTF-8")
        })
    }
}
