from redis import Redis
from rq import Queue
import test1

rconn = Redis(host='127.0.0.1', port=11003)

queue = Queue(connection=rconn)
job = queue.enqueue(test1.say)


"""
The expected 'data' in RQ is as follows:

x\x9ck`\x9d*\xc2\x00\x01\x1a=\x9c%\xa9\xc5%\x86z\xc5\x89\x95S\xfc4k\xa7\x94L\xd1\x03\x00mQ\x08\xaa
"""
