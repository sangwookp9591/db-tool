"""Test-only SOCKS5/HTTP CONNECT proxy; bind only through Docker's loopback publish."""
import select
import socket
import socketserver
import struct


def receive(stream, size):
    data = b""
    while len(data) < size:
        chunk = stream.recv(size - len(data))
        if not chunk:
            raise EOFError()
        data += chunk
    return data


class Proxy(socketserver.BaseRequestHandler):
    def handle(self):
        source = self.request
        source.settimeout(20)
        try:
            first = receive(source, 1)
            if first == b"\x05":
                methods = receive(source, receive(source, 1)[0])
                if 0 not in methods:
                    source.sendall(b"\x05\xff")
                    return
                source.sendall(b"\x05\x00")
                version, command, _, kind = receive(source, 4)
                if version != 5 or command != 1:
                    return
                if kind == 1:
                    host = socket.inet_ntop(socket.AF_INET, receive(source, 4))
                elif kind == 4:
                    host = socket.inet_ntop(socket.AF_INET6, receive(source, 16))
                elif kind == 3:
                    host = receive(source, receive(source, 1)[0]).decode("ascii")
                else:
                    return
                port = struct.unpack("!H", receive(source, 2))[0]
                response = b"\x05\x00\x00\x01\x7f\x00\x00\x01\x00\x00"
            else:
                header = first
                while not header.endswith(b"\r\n\r\n"):
                    if len(header) >= 16384:
                        return
                    header += receive(source, 1)
                method, authority, _ = header.split(b"\r\n", 1)[0].decode("ascii").split()
                if method != "CONNECT":
                    return
                host, port = authority.rsplit(":", 1)
                host, port = host.strip("[]"), int(port)
                response = b"HTTP/1.1 200 Connection Established\r\n\r\n"
            # The fixture can reach only the explicitly authorized test database ports.
            if host != "host.docker.internal" or port not in (55432, 55433, 57017, 56379, 55222):
                return
            with socket.create_connection((host, port), timeout=10) as destination:
                source.sendall(response)
                source.settimeout(None)
                destination.settimeout(None)
                while True:
                    readable, _, _ = select.select([source, destination], [], [], 30)
                    if not readable:
                        return
                    for stream in readable:
                        data = stream.recv(65536)
                        if not data:
                            return
                        (destination if stream is source else source).sendall(data)
        except (OSError, EOFError, ValueError):
            return


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


if __name__ == "__main__":
    with Server(("0.0.0.0", 1080), Proxy) as server:
        server.serve_forever()
