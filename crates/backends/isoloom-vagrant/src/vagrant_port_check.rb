# Vagrant's own check of a host port before forwarding it (`Vagrant::Util::IsPortOpen`) asks
# `Socket.tcp` with a timeout, and on macOS 27 that hands back a socket for a refused connection
# (Ruby bug #22223: the retried `connect_nonblock` answers EISCONN while the socket's SO_ERROR
# still holds ECONNREFUSED). Every host port then reads as in use, and a VM's fixed forwarded
# port fails as a collision before the VM boots (the SSH one Vagrant moves, to a port that read
# as free by luck). On a Mac the check reads the socket's error itself, as Vagrant's should.
if RUBY_PLATFORM.include?("darwin") && defined?(Vagrant)
  require "vagrant/util/is_port_open"

  module Vagrant::Util::IsPortOpen
    def is_port_open?(host, port)
      addr = Addrinfo.tcp(host, port)
      sock = Socket.new(addr.afamily, addr.socktype, addr.protocol)
      begin
        if sock.connect_nonblock(addr, exception: false) == :wait_writable
          return false unless sock.wait_writable(0.1)
        end
        sock.getsockopt(Socket::SOL_SOCKET, Socket::SO_ERROR).int.zero?
      rescue SystemCallError
        false
      ensure
        sock.close
      end
    end
  end
end
