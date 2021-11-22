### Crate Options

Interprocess: https://crates.io/crates/interprocess
Metal IO:     https://github.com/tokio-rs/mio

#### Functions of the Daemon

1. Listen on a Unix Domain Socket for BTU Task Schedule Identifiers.
	* Each is added to an internal queue.
2. Wait N seconds, query SQL, and append All Identifiers into the internal queue.
3. Pop values from the internal queue, fetch corresponding SQL data, and update the Python Redis Queue accordingly.

All 3 should run concurrently.
Because all 3 are "sharing" this internal queue?  We need to handle Concurrency + Shared State.

https://doc.rust-lang.org/book/ch16-03-shared-state.html


#### Thread 1: Unix Domain Socket

* This process binds its socket to a known location and accepts incoming  connection requests from clients. 
* For each connection request that is received, a new socket is created that is used to communicate with the peer socket (*peer socket* = the socket at the other end of the connection, in this case the socket created by some client process).

#### Thread 1: Unix Domain Socket

* This process binds its socket to a known location and accepts incoming  connection requests from clients. 
* For each connection request that is received, a new socket is created that is used to communicate with the peer socket (*peer socket* = the socket at the other end of the connection, in this case the socket created by some client process).

#### Thread 1: Unix Domain Socket

* This process binds its socket to a known location and accepts incoming  connection requests from clients. 
* For each connection request that is received, a new socket is created that is used to communicate with the peer socket (*peer socket* = the socket at the other end of the connection, in this case the socket created by some client process).

### Other artifcats
#### Internal Queue

* Basically, a Vector of String.  Where each String represents a BTU Task Schedule that should be written to Python RQ.
* But instead of `Vector<String>`, I've opted to use `VecDeque`

How is this queue filled?

* Once, when the daemon is called.
* Every N minutes, the daemon performs a "full refresh" using latest SQL rows in `tabBTU Task Schedule`
* If the daemon's Unix Domain Socket gets a call from a client, with a BTU Task Schedule Identifier.

TODO: Would be great if the queue was a unique set of values (no duplicate strings).  

