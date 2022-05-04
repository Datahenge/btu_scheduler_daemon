### Purpose and Functions of the Daemon

1. Listen on a Unix Domain Socket for BTU Task Schedule Identifiers.
	* Each is added to an internal queue.
2. Wait N seconds, query SQL, and append All Identifiers into the internal queue.
3. Pop values from the internal queue, fetch corresponding SQL data, and update the Python Redis Queue accordingly.

All 3 should run concurrently.
Because all 3 are "sharing" this internal queue?  We need to handle Concurrency + Shared State.

https://doc.rust-lang.org/book/ch16-03-shared-state.html


### Threads
This is a multi-threaded, concurrent application.  Note that it is **not** an *async* application.

#### Main Thread: Unix Domain Socket listener

* This process binds its socket to a known location and accepts incoming  connection requests from clients. 
* For each connection request that is received, a new socket is created that is used to communicate with the peer socket (*peer socket* = the socket at the other end of the connection, in this case the socket created by some client process).

##### Message Format
Client messages to the socket server are UTF-8 strings, which represent JSON as follows:
```
{
    'request_type': request_type.name,
    'request_content': content
}
```

As an example, the socket Client might send a 'ping' request, which looks like this:
```
{
    'request_type': 'ping',
    'request_content': None
}
```

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

* Basically, a Vector of String.  Where each String represents a `BTU Task Schedule` identifier, that should be written to Python RQ.
* Note that instead of `Vector<String>`, I've opted to use `VecDeque`

How is this queue filled?

* On the initial startup of the BTU Scheduler daemon.
* Every N minutes, the daemon performs a "full refresh" using latest SQL rows in `tabBTU Task Schedule`
* If the daemon's Unix Domain Socket listener gets a call from a client, with a BTU Task Schedule Identifier.

TODO: Would be nice if the queue was a unique set of values (no duplicate Task Schedule strings)

## Frappe Web Server Endpoints
When installed on a Frappe site, the **BTU App** exposes the following HTTP endpoints for the BTU Scheduler:

* `get_pickled_task(task_id)`
* `test_ping()`

##### Deprecated?
* `test_hello_world_bytes()`
* `test_function_ping_now_bytes()`

### Questions and Answers
Q: Does btu-daemon need to run as root?
A: Yes, because we're storing credentials (email, Frappe login token) in root-owned files with permissions = 600

Q: Isn't that insecure?
A: Only if someone gains root access to your server.  In which case, they have access to everything anyway.

Q: You could encrypt the passwords.
A: Yes, but BTU runs as an unattended daemon.  It would have to be able to decrypt the passwords on its own.  Which means storing the keys or hashes
on the server too.  You might slow down an attacker for a minute or two.  That's all.

Q: You could require a human enter a password when initially starting the daemon.
A: I could.  But if the server reboots or the power cycles, then the BTU will not startup automatically.  It would require a human to login and kickstart it.
