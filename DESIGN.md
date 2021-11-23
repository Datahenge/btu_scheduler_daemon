#### Purpose and Functions of the Daemon

1. Listen on a Unix Domain Socket for BTU Task Schedule Identifiers.
	* Each is added to an internal queue.
2. Wait N seconds, query SQL, and append All Identifiers into the internal queue.
3. Pop values from the internal queue, fetch corresponding SQL data, and update the Python Redis Queue accordingly.

All 3 should run concurrently.
Because all 3 are "sharing" this internal queue?  We need to handle Concurrency + Shared State.

https://doc.rust-lang.org/book/ch16-03-shared-state.html


### Threads
#### Main Thread: Unix Domain Socket listener

* This process binds its socket to a known location and accepts incoming  connection requests from clients. 
* For each connection request that is received, a new socket is created that is used to communicate with the peer socket (*peer socket* = the socket at the other end of the connection, in this case the socket created by some client process).

#### Sub-Thread 1: Internal Queue Consumer

* Pops string values from the deamon's internal queue.  These strings represents BTU Task Scheduler `name` values from the BTU App (Frappe framework)
* For each string, read the corresponding SQL row in table `tabBTU Task Scheduler`
    * Save the SQL row data in a Rust struct `BtuTaskScheduler`
* Parse the data.  Using the cron string, calculate the Next Run Date.
* Store the Python function in Redis queue as a Job.

#### Sub-Thread 2: Internal Queue Refiller

* Every N seconds, read **all** the rows in SQL table `tabBTU Task Scheduler`
* For each row, add the `name` to the internal queue.

The net result is a kind of *"automatic, full synchronization refresh."*
No matter the status of the Frappe web application, the daemon ensure that every N seconds, the BTU Task Schedules are fully-synchronized into the Python RQ database.

**Note**: This same full-refresh also happens immediately on daemon startup, but inside the main thread.

#### Sub-Thread 3: Scheduler & Timer
This thread effectively replaces the functionality in the excellent [rq-scheduler](https://github.com/rq/rq-scheduler/) library:

### Other artifacts
#### Internal Queue

* Basically, a Vector of String.  Where each String represents a BTU Task Schedule that should be written to Python RQ.
* But instead of `Vector<String>`, I've opted to use `VecDeque`

How is this queue filled?

* Once, when the daemon is called.
* Every N minutes, the daemon performs a "full refresh" using latest SQL rows in `tabBTU Task Schedule`
* If the daemon's Unix Domain Socket gets a call from a client, with a BTU Task Schedule Identifier.

### TODO List

* Would be great if the queue was a unique set of values (no duplicate strings)
