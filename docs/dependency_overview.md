### Dependency Documentation

The following provides a brief explanation of the third-party crates I used from https://crates.io

#### anyhow
https://docs.rs/anyhow

A lovely crate that simplifies Error Handling in Rust.  Especially useful for higher-level applications, where strict Error typing isn't a requirement.


#### chrono
https://docs.rs/chrono

Direct quote from the web page above:
```
Date and time handling for Rust. It aims to be a feature-complete superset of the time library. In particular,

    Chrono strictly adheres to ISO 8601.
    Chrono is timezone-aware by default, with separate timezone-naive types.
    Chrono is space-optimal and (while not being the primary goal) reasonably efficient.
```

#### cron
https://docs.rs/cron

This creates provides 7-element cron parsing functionality.  I had to add a bit of extra logic to my library, to transform 5-6 element cron expressions into 7 elements.


#### mysql
https://docs.rs/mysql

This library provides the mechanism to read from the MySQL table `tabBTU Task Schedule`

#### once_cell
https://docs.rs/once_cell

This crate provides the concept of 'Lazy Statics'.  It's used to create the concept of global configuration for the daemon.

```rust
static GLOBAL_CONFIG: Lazy<Mutex<AppConfig>> = Lazy::new(|| {
    Mutex::new(AppConfig::new_from_toml_file())
});
```

#### redis
https://docs.rs/redis

This library provides the mechanism to read and write to the ERPNext Redis Queue, which is acting as a Python RQ database.


#### serde = { version = "1.0.130", features = ["derive"] }
https://docs.rs/serde

This library (besides being a dependency of most of the others documented here), is enabling the `Deserialize` trait for the Application Configuration.

```rust
#[derive(Deserialize)]
pub struct AppConfig {
	...
}
```

#### thiserror
https://docs.rs/thiserror

A lovely crate that helps coerce between different Error types.  This allows you to return a "unified" Error type of enum, which encapsulates errors from other crate libraries (plus your own), and also allows you to annotate them.

#### toml
https://docs.rs/toml

I really love TOML.  It's fantastic for storing configuration data: which is precisely what it's doing here.
The local, hidden file `.py_schedule.toml` contains connection information for MySQL and Redis, and a few other parameters.
This library enables the daemon to read that information into the `AppConfig` struct

### Other crates I gave consideration to.

I thought about these, but they didn't make the cut.

* Interprocess: https://crates.io/crates/interprocess
* Metal IO:     https://github.com/tokio-rs/mio


## Other Notes

### The Many Troubles of Cron

1. The 'cron' crate assumes all cron expressions are in terms of UTC time.
2. The 'cron' crate works with a 7-element cron expression.  Most Unix systems only recognize 5-elements.
3. The 'cron' crate believes that Sunday is day 1 and Saturday is day 7.  This is contrary to other software (Sunday=0, Saturday=6)
4. How do you convert from a TZ-aware cron expression to a UTC cron expression?

#### cron5 versus cron7

The Rust 'cron' library expects a 7-element cron string.  Where the additional elements are:
* Seconds (position 0)
* Years (position 6)

So that overall, the cron expression looks like this:
```
Seconds    Minutes    Hours    Day_of_Month    Month    Day_of_Week    Years
```

To integrate with this library, I've had to expand cron expressions with fewer elements, into 7 elements.
The function handling this is `cron_str_to_cron_str7()` in the `btu_cron.rs` file.
