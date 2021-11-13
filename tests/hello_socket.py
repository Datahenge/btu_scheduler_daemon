""" hello_socket.py """

#!/bin/python3

import socket
import time

# Create a Unix Domain Socket (client side)

server_address = "/tmp/pyrq_scheduler.sock"  # pylint: disable=invalid-name

def talk_to_server():
	"""
	Send a few strings (as bytes) to the remote Unix Domain Socket server.
	"""
	sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
	
	# Connect the socket to the port where the server is listening
	sock.connect(server_address)

	def send_message(message_string):
		print(f"Sending '{message_string}' to server...")
		message = bytes(message_string, 'utf-8')
		sock.sendall(message)

	send_message("Hello, Christine!")
	time.sleep(5)
	send_message("I am a little daemon!")
	time.sleep(1)

	print("Closing this client socket, and exiting...")
	sock.close()

if __name__ == '__main__':
	talk_to_server()
