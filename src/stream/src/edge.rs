use std::marker::PhantomData;

use common::{
    event::{LocalEvent, StreamEvent},
    net::gateway::taskmanager::SafeTaskManagerRpcGateway,
};
use tonic::async_trait;

use crate::{Receiver, Sender};

#[async_trait]
pub trait OutEdge: Send + Sync {
    type Output;

    async fn write(&self, val: Self::Output) -> Result<(), OutEdgeError>;

    async fn batch_write(&self, iter: Vec<Self::Output>) -> Result<(), OutEdgeError>;
}

pub struct LocalOutEdge<T> {
    tx: Sender<bytes::Bytes>,
    _data_type: PhantomData<T>,
}

unsafe impl<T> Send for LocalOutEdge<T> {}
unsafe impl<T> Sync for LocalOutEdge<T> {}

impl<T> LocalOutEdge<T> {
    pub fn new(tx: Sender<bytes::Bytes>) -> Self {
        Self {
            tx,
            _data_type: PhantomData,
        }
    }
}

#[async_trait]
impl<T: StreamEvent> OutEdge for LocalOutEdge<T> {
    type Output = T;

    async fn write(&self, val: T) -> Result<(), OutEdgeError> {
        let mut buf = vec![];
        let mut serializer = rmp_serde::Serializer::new(&mut buf);
        val.serialize(&mut serializer)
            .map_err(|err| OutEdgeError::from(err))?;

        self.tx
            .send(bytes::Bytes::from(buf))
            .await
            .map_err(|err| OutEdgeError::SendToLocalFailed(err.to_string()))
    }

    async fn batch_write(&self, iter: Vec<Self::Output>) -> Result<(), OutEdgeError> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum OutEdgeError {
    SendToLocalFailed(String),
    SendToRemoteFailed(tonic::Status),
    EncodeError(rmp_serde::encode::Error),
}

impl From<rmp_serde::encode::Error> for OutEdgeError {
    fn from(err: rmp_serde::encode::Error) -> Self {
        Self::EncodeError(err)
    }
}

impl ToString for OutEdgeError {
    fn to_string(&self) -> String {
        match self {
            OutEdgeError::SendToLocalFailed(message) => format!("SendToLocalFailed: {}", message),
            OutEdgeError::SendToRemoteFailed(status) => format!("SendToRemoteFailed: {}", status),
            OutEdgeError::EncodeError(err) => format!("RmpEncodeError: {}", err),
        }
    }
}

pub struct RemoteOutEdge {
    gateway: SafeTaskManagerRpcGateway,
}

impl RemoteOutEdge {
    pub fn new(gateway: SafeTaskManagerRpcGateway) -> Self {
        Self { gateway }
    }
}

unsafe impl Send for RemoteOutEdge {}
unsafe impl Sync for RemoteOutEdge {}

#[async_trait]
impl OutEdge for RemoteOutEdge {
    type Output = LocalEvent;

    async fn write(&self, val: LocalEvent) -> Result<(), OutEdgeError> {
        match val {
            LocalEvent::Terminate { .. } => Ok(()),
            LocalEvent::KeyedDataStreamEvent(event) => self
                .gateway
                .send_event_to_operator(event)
                .await
                .map(|_| ())
                .map_err(|err| OutEdgeError::SendToRemoteFailed(err)),
        }
    }

    async fn batch_write(&self, iter: Vec<Self::Output>) -> Result<(), OutEdgeError> {
        Ok(())
    }
}

/// The trait that represents an in-edge
#[async_trait]
pub trait InEdge: Send + Sync + Unpin {
    type Output;

    async fn receive_data_stream(&mut self) -> Option<Self::Output>;

    fn poll_recv_data_stream(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Output>>;
}

pub struct LocalInEdge<T> {
    rx: Receiver<bytes::Bytes>,
    _data_type: PhantomData<T>,
}

impl<T> Drop for LocalInEdge<T> {
    fn drop(&mut self) {
        self.rx.close()
    }
}

unsafe impl<T> Send for LocalInEdge<T> {}
unsafe impl<T> Sync for LocalInEdge<T> {}
impl<T> Unpin for LocalInEdge<T> {}

#[async_trait]
impl<T: StreamEvent> InEdge for LocalInEdge<T> {
    type Output = T;

    async fn receive_data_stream(&mut self) -> Option<T> {
        self.rx.recv().await.and_then(|buf| {
            T::from_slice(&buf)
                .map_err(|err| tracing::error!("{:?}", err))
                .ok()
        })
    }

    fn poll_recv_data_stream(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Output>> {
        match self.rx.poll_recv(cx) {
            std::task::Poll::Ready(buf) => std::task::Poll::Ready(buf.and_then(|buf| {
                T::from_slice(&buf)
                    .map_err(|err| tracing::error!("{:?}", err))
                    .ok()
            })),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl<T> LocalInEdge<T> {
    pub fn new(rx: Receiver<bytes::Bytes>) -> Self {
        Self {
            rx,
            _data_type: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use common::event::LocalEvent;
    use proto::common::KeyedDataEvent;

    use crate::{edge::InEdge, new_event_channel};

    use super::{LocalInEdge, LocalOutEdge, OutEdge};

    #[tokio::test]
    async fn test_local_edge_success() {
        let (tx, rx) = new_event_channel(10);

        let mut in_edge = LocalInEdge::<LocalEvent>::new(rx);
        let out_edge = LocalOutEdge::<LocalEvent>::new(tx);

        let result = out_edge
            .write(LocalEvent::KeyedDataStreamEvent(KeyedDataEvent::default()))
            .await;
        assert!(result.is_ok());

        let opt = in_edge.receive_data_stream().await;
        assert!(opt.is_some());
    }
}