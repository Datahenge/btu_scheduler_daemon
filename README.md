## BTU Scheduler Daemon

### Purpose

The purpose of this program is to create a 64-bit Linux binary executable, that run as a daemon.

1. Initializes scheduled tasks (*for now, Python functions*) into a Redis Queue.
2. Listens for schedule changes over a Unix Domain Socket.
3. Periodically rereads the schedule from the 'system of record' (e.g. every 15 minutes)

### Why?
Read [here](WHY.md) for more about why I needed to create this application.

### Additional Features

* Ships with a companion CLI application, that you can use to ask the daemon about its current status.
* Reads Task Schedules:
  * from MySQL database tables.
  * JSON and TOML files.
* Print activity to:
  * standard output.
  * log files.

#### See also:
https://github.com/Couragium/rsmq-async-rs


# License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.
