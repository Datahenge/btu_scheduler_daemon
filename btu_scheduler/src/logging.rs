// logging.rs

/*
	NOTE: This info!() syntax is compatible with my Levels and Subscriber:

		let user = "ferris";
		info!(
			target: "connection_events",
			key_str = "value",
			key_int = 1,
			user
		);

	This is NOT compatible:

	let datetime = Utc::now();		
	let owned_string =  "baz".to_owned();
	let my_vec: Vec<&str> =  vec!["one", "two", "three"];
	info!(
		target: "connection_events",
		yes = owned_string,
	);
*/

use std::fmt;
use serde::ser::SerializeTuple;
use serde::{Serialize, Serializer, Deserialize, Deserializer};
use serde::de::{self, Visitor};
use tracing::Level;
use tracing_subscriber::filter::LevelFilter;

pub struct LevelWrapper ( pub tracing::Level );  // tuple struct: See article https://rust-unofficial.github.io/patterns/patterns/behavioural/newtype.html

impl LevelWrapper {
	pub fn new(level: tracing::Level) -> LevelWrapper {
		LevelWrapper(level)
	}
	pub fn get_level(&self) -> Level {
		self.0
	}
}

struct LevelWrapperVisitor;
// A Visitor is instantiated by a Deserialize impl and passed to a Deserializer. The Deserializer then calls a method on the Visitor in order to construct the desired type.
impl<'de> Visitor<'de> for LevelWrapperVisitor {
    type Value = LevelWrapper;  // this is the type I'm trying to -create-

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string representing a Level enum from the tracing crate.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
		let result_level: LevelWrapper = match value {
			"TRACE" => LevelWrapper(Level::TRACE),
			"DEBUG" => LevelWrapper(Level::DEBUG),
			"INFO" => LevelWrapper(Level::INFO),
			"WARN" => LevelWrapper(Level::WARN),
			"ERROR" => LevelWrapper(Level::ERROR),
			_ => panic!("Unrecognized level value: {}", value),
		};
        Ok(result_level)
    }
}

impl Serialize for LevelWrapper {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
		where S: Serializer
	{
		let mut tup = serializer.serialize_tuple(1)?;
		tup.serialize_element(&self.0.to_string())?;  // Unsure if this is reasonable, but converting the Level to a string seems the easiest approach to Serialization.
		tup.end()
	}
}

impl<'a> Deserialize<'a> for LevelWrapper {
	fn deserialize<'de, D>(deserializer: D) -> Result<Self, D::Error>
		where D: Deserializer<'a>
	{
		deserializer.deserialize_str(LevelWrapperVisitor)
	}
}

// Next, implement Serialize and Deserial for tracing_level: filter::LevelFilter

pub struct LevelFilterWrapper ( pub LevelFilter);  // tuple struct: See article https://rust-unofficial.github.io/patterns/patterns/behavioural/newtype.html

impl LevelFilterWrapper {
	pub fn new(level_filter: LevelFilter) -> LevelFilterWrapper {
		LevelFilterWrapper(level_filter)
	}
	pub fn get_level(&self) -> LevelFilter {
		self.0
	}
}

struct LevelFilterWrapperVisitor;
// A Visitor is instantiated by a Deserialize impl and passed to a Deserializer. The Deserializer then calls a method on the Visitor in order to construct the desired type.
impl<'de> Visitor<'de> for LevelFilterWrapperVisitor {
    type Value = LevelFilterWrapper;  // this is the type I'm trying to -create-

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string representing a Level enum from the tracing crate.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
		let result_level: LevelFilterWrapper = match value {
			"TRACE" => LevelFilterWrapper(LevelFilter::TRACE),
			"DEBUG" => LevelFilterWrapper(LevelFilter::DEBUG),
			"INFO" => LevelFilterWrapper(LevelFilter::INFO),
			"WARN" => LevelFilterWrapper(LevelFilter::WARN),
			"ERROR" => LevelFilterWrapper(LevelFilter::ERROR),
			_ => panic!("Unrecognized level value: {}", value),
		};
        Ok(result_level)
    }
}

impl Serialize for LevelFilterWrapper {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
		where S: Serializer
	{
		let mut tup = serializer.serialize_tuple(1)?;
		tup.serialize_element(&self.0.to_string())?;  // Unsure if this is reasonable, but converting the Level to a string seems the easiest approach to Serialization.
		tup.end()
	}
}

impl<'a> Deserialize<'a> for LevelFilterWrapper {
	fn deserialize<'de, D>(deserializer: D) -> Result<Self, D::Error>
		where D: Deserializer<'a>
	{
		deserializer.deserialize_str(LevelFilterWrapperVisitor)
	}
}
