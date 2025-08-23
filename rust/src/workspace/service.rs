use std::any::Any;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use tokio::sync::mpsc;

pub trait Service: Send + Sized + 'static {
    fn run(mut self) -> Client<Self> {
        let (tx, mut rx) = mpsc::channel::<Box<dyn Envelope>>(16);
        tokio::spawn(async move {
            while let Some(envelope) = rx.recv().await {
                let service = &mut self as &mut dyn Any;
                envelope.handle_envelope(service).await;
            }
        });

        Client {
            tx,
            _phantom: PhantomData,
        }
    }

    /// Spawn a service with self-reference support
    /// This is useful when a service needs to send messages to itself
    fn spawn_with_self<F>(mut self, init: F) -> Client<Self>
    where
        F: FnOnce(&mut Self, Client<Self>),
    {
        let (tx, mut rx) = mpsc::channel::<Box<dyn Envelope>>(16);
        let client = Client {
            tx: tx.clone(),
            _phantom: PhantomData,
        };

        // Initialize the service with its own client
        init(&mut self, client.clone());

        tokio::spawn(async move {
            while let Some(envelope) = rx.recv().await {
                let service = &mut self as &mut dyn Any;
                envelope.handle_envelope(service).await;
            }
        });

        Client {
            tx,
            _phantom: PhantomData,
        }
    }
}

// MessageトレイトなしでHandlerを定義
pub trait Handler<M>: Service
where
    M: Send + 'static,
{
    type Response: Send + 'static;

    fn handle(&mut self, msg: M) -> impl Future<Output = Self::Response> + Send;
}

trait Envelope: Send {
    fn handle_envelope(
        self: Box<Self>,
        service: &mut dyn Any,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

struct MessageEnvelope<S, M>
where
    S: Service + Handler<M>,
    M: Send + 'static,
{
    message: M,
    response_tx: Option<tokio::sync::oneshot::Sender<S::Response>>,
    _phantom: PhantomData<S>,
}

impl<S, M> Envelope for MessageEnvelope<S, M>
where
    S: Service + Handler<M>,
    M: Send + 'static,
{
    fn handle_envelope(
        self: Box<Self>,
        service: &mut dyn Any,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        let service = service
            .downcast_mut::<S>()
            .expect("Service type mismatch in envelope");

        let message = self.message;
        let response_tx = self.response_tx;

        Box::pin(async move {
            let result = Handler::<M>::handle(service, message).await;
            if let Some(tx) = response_tx {
                let _ = tx.send(result);
            }
        })
    }
}

pub struct Client<S: Service> {
    tx: mpsc::Sender<Box<dyn Envelope>>,
    _phantom: PhantomData<S>,
}

impl<S: Service> Clone for Client<S> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            _phantom: PhantomData,
        }
    }
}

impl<S: Service> Client<S> {
    pub async fn request<M>(&self, msg: M) -> S::Response
    where
        M: Send + 'static,
        S: Handler<M>, // Handlerが実装されていれば送信可能
    {
        let (tx, rx) = tokio::sync::oneshot::channel();

        let envelope = Box::new(MessageEnvelope::<S, M> {
            message: msg,
            response_tx: Some(tx),
            _phantom: PhantomData,
        });

        self.tx
            .send(envelope)
            .await
            .expect("Failed to send message to actor");

        rx.await.expect("Failed to receive response from actor")
    }
}
