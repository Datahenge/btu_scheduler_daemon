### Crate Options

Interprocess: https://crates.io/crates/interprocess
Metal IO:     https://github.com/tokio-rs/mio





**Server Process (AKA the server)**

* This process binds its socket to a known location and accepts incoming  connection requests from clients. 

* For each connection request that is received, a new socket is created that is used to communicate with the peer socket (*peer socket* = the socket at the other end of the connection, in this case the socket created by some client process).
* 



#### Queue of Task Schedule ID's that need refreshing.
How is this queue filled?
* Every N minutes, the daemon fills it with All Schedule ID values from `tabBTU Task Schedule`
* The TCP socket server can add a value.

The queue should be unique.
There is no point in repeating the same Task Value twice.

