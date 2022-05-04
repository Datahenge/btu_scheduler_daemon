// logging.rs

use std::fmt;
use serde::ser::SerializeTuple;
use serde::{Serialize, Serializer, Deserialize, Deserializer};
use serde::de::{self, Visitor};
use tracing::Level;

struct MyFancyVisitor;

pub struct LevelWrapper ( pub tracing::Level );  // tuple struct: See article https://rust-unofficial.github.io/patterns/patterns/behavioural/newtype.html

impl LevelWrapper {
	pub fn new(level: tracing::Level) -> LevelWrapper {
		LevelWrapper(level)
	}
	pub fn get_level(&self) -> Level {
		self.0
	}
}

// A Visitor is instantiated by a Deserialize impl and passed to a Deserializer. The Deserializer then calls a method on the Visitor in order to construct the desired type.
impl<'de> Visitor<'de> for MyFancyVisitor {
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
		deserializer.deserialize_str(MyFancyVisitor)
	}
}
