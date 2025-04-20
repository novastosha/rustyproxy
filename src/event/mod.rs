use crate::{packet::RawPacket, player::{PlayerConnection, PlayerProxyConnection}, server::ProxiedServer, ProxyInstance, SharedProxyInstance};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::Arc,
};
use tokio::sync::{Mutex, RwLock};

#[derive(Clone, PartialEq, Eq)]
pub enum EventResult {
    Continue,
    Stop,
}

#[derive(Clone, PartialEq, Eq)]
pub enum NoopEventResult {}

pub trait Event<R: Clone + Send + Sync + 'static>: Send + Sync + Clone + 'static {}

pub struct EventBus {
    listeners: RwLock<
        HashMap<
            TypeId,
            Vec<(
                bool,
                Box<
                    dyn Fn(
                            SharedProxyInstance,
                            Arc<RwLock<Box<dyn Any + Send + Sync>>>,
                        ) -> Pin<
                            Box<dyn Future<Output = Option<Box<dyn Any + Send + Sync>>> + Send>,
                        > + Send
                        + Sync,
                >,
            )>,
        >,
    >,

    instance: SharedProxyInstance,
}

impl EventBus {
    pub fn new(instance: &SharedProxyInstance) -> Arc<Self> {
        Arc::new(Self {
            listeners: RwLock::new(HashMap::new()),
            instance: instance.clone(),
        })
    }

    /// **Register an async event listener with a `lazy` flag**
    pub async fn listen<E,R, F, Fut>(&self, lazy: bool, callback: F)
    where
    E: Event<R>,
    R: Clone + Send + Sync + 'static,
        F: Fn(SharedProxyInstance, Arc<E>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<R>> + Send + 'static,
    {
        let mut listeners = self.listeners.write().await;
        listeners
            .entry(TypeId::of::<E>())
            .or_insert_with(Vec::new)
            .push((
                lazy,
                Box::new({
                    let callback = Arc::new(callback);
                    move |instance, event| {
                        let callback = Arc::clone(&callback);
                        Box::pin(async move {
                            let event = event.read().await;
                            if let Some(event) = event.downcast_ref::<Arc<E>>() {
                                callback(instance, event.clone())
                                    .await
                                    .map(|res| Box::new(res) as Box<dyn Any + Send + Sync>)
                            } else {
                                None // Default to None if downcast fails
                            }
                        })
                    }
                }),
            ));
    }

    /// **Dispatch an event, awaiting non-lazy listeners**
    pub async fn dispatch<E: Event<R>,R: Clone + Send + Sync + 'static,>(&self, event: &Arc<E>) -> Option<R> {
        let listeners = self.listeners.read().await;
        let event: Arc<RwLock<Box<dyn Any + Send + Sync>>> = Arc::new(RwLock::new(Box::new(event.clone())));
    
        let mut last_result = None;
        let mut tasks = Vec::new();
    
        for (lazy, handler) in listeners.get(&TypeId::of::<E>()).into_iter().flatten() {
            let instance = self.instance.clone();
            let event = Arc::clone(&event);
    
            if *lazy {
                tokio::spawn(handler(instance, event));
            } else {
                tasks.push(handler(instance, event));
            }
        }
    
        for task in tasks {
            if let Some(result_boxed) = task.await {
                if let Ok(result) = result_boxed.downcast::<R>() {
                    last_result = Some(*result);
                }
            }
        }
    
        last_result
    }
}

pub trait PlayerEvent: Event<EventResult> {
    fn mutate_player(&self, player: &mut PlayerConnection);
    fn player(&self) -> &Arc<Mutex<PlayerConnection>>;
}

#[derive(Clone)]
pub struct ProxyFinishedInitialization;
impl Event<EventResult> for ProxyFinishedInitialization {}

#[derive(Clone)]
pub struct PlayerJoinedProxy {
    pub connection: Arc<tokio::sync::Mutex<PlayerConnection>>,
}
impl Event<EventResult> for PlayerJoinedProxy {}

#[derive(Clone)]
pub struct PlayerJoinedServer {
    pub connection: Arc<tokio::sync::Mutex<PlayerConnection>>,
    pub server: Arc<ProxiedServer>,
}
impl Event<EventResult> for PlayerJoinedServer {}


#[derive(Clone)]
pub struct PlayerLeftProxy {
    pub connection: Arc<tokio::sync::Mutex<PlayerConnection>>,
    pub server: Option<Arc<ProxiedServer>>,
}
impl Event<NoopEventResult> for PlayerLeftProxy {}

#[derive(Clone)]
pub struct ServerSentPacket {
    pub connection: Arc<tokio::sync::Mutex<PlayerConnection>>,
    pub packet: RawPacket
}

impl Event<EventResult> for ServerSentPacket {}