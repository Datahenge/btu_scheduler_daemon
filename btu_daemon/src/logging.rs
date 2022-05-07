/*
	Further Reading
	Creating Spans: https://docs.rs/tracing/latest/tracing/span/index.html
	https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/index.html
*/

/* The Levels of tracing:

    0:  Trace.  Designates very low priority, often extremely verbose, information.
    1:  Debug:  Designates lower priority information.
    2:  Info:   Designates useful information.
    3:  Warn:   Designates hazardous situations.
    4:  Error:  Designates very serious errors.
*/

use std::collections::BTreeMap;
use chrono::{SecondsFormat, Utc};
use tracing::Subscriber;
use tracing_subscriber::Layer;


// Again with the Visitor Pattern.  :thinking:
pub struct CustomLayer;

impl<S> Layer<S> for CustomLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Covert the values into a JSON object
        let mut fields = BTreeMap::new();
        let mut visitor = JsonVisitor(&mut fields);
        event.record(&mut visitor);

        // Output the event in JSON
        let output = serde_json::json!({
			"timestamp":  Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            // "target": event.metadata().target(),
            "name": event.metadata().name(),
            "level": format!("{:?}", event.metadata().level()),
            "fields": fields,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());

        // TODO: Send an email on certain Levels (e.g. email when WARN or ERROR)
    }
}

// Again, I find this Visitor Pattern.  :thinking:
struct JsonVisitor<'a>(&'a mut BTreeMap<String, serde_json::Value>);

impl<'a> tracing::field::Visit for JsonVisitor<'a> {
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0
            .insert(field.name().to_string(), serde_json::json!(value));
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.0.insert(
            field.name().to_string(),
            serde_json::json!(value.to_string()),
        );
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.insert(
            field.name().to_string(),
            serde_json::json!(format!("{:?}", value)),
        );
    }
}
