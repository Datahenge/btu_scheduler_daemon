// errors.rs

use toml::de::Error		as TomlError;
use thiserror::Error	as ThisError;

#[derive(ThisError, Debug)]
pub enum ConfigError {
	#[error("Could not deserialize TOML into a Rust object.\n    {source:?}")]
	ConfigLoad {
		#[from] 
		source: TomlError,
	},
	#[error("Cannot find the TOML configuration file on disk.")]
	MissingConfigFile,
}

#[derive(ThisError, Debug, PartialEq)]
pub enum CronError {
	#[error("Cron expression has the wrong number of elements (should be one of 5, 6, or 7).")]
	WrongQtyOfElements {
		found: usize
	},
	#[error("Invalid cron expression; could not transform into a CronStruct.")]
	InvalidExpression
}

#[derive(ThisError, Debug, PartialEq)]
pub enum StringError {
	#[error("Element cannot be split using delimiter.")]
	MissingDelimiter,
}

#[derive(ThisError, Debug, PartialEq)]
pub enum RQError {
	#[error("No idea what happened here.")]
	Unknown {
		#[from]
		source: redis::RedisError,
	}
}

// Email Errors
#[derive(ThisError, Debug, PartialEq)]
pub enum EmailConfigError {
	#[error("Cron expression has the wrong number of elements (should be one of 5, 6, or 7).")]
	WrongQtyOfElements {
		found: usize
	},
	#[error("Invalid cron expression; could not transform into a CronStruct.")]
	InvalidExpression
}
