use tracing_subscriber::Layer;

pub struct CustomLayer;

impl<S> Layer<S> for CustomLayer where S: tracing::Subscriber {}




/*
Further Reading

Creating Spans: https://docs.rs/tracing/latest/tracing/span/index.html

*/