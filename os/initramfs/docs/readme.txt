AmbaOS v0.5 & AmbaDE Quick Guide
=================================

Specifications & Networking:
- Default QEMU RAM: 2 GiB (-m 2G) with PMM up to 16 GiB & Kernel Heap 256 MiB.
- Virtual NIC: Intel E1000 (-nic user,model=e1000) with automatic DHCP,
  ICMP Ping, DNS Resolver, and HTTP/1.1 TCP Client (smoltcp).

Shell Commands in AmbaTerm:
- ip / ifconfig   : Show MAC, DHCP IPv4, Gateway, DNS, & RX/TX packets
- ping <ip>       : Send ICMP Echo Request (default 10.0.2.2)
- dns <domain>    : Resolve hostname to IPv4
- httpget <url>   : Fetch HTTP/1.1 page & display in AmbaTerm + AmbaBrowser
- browser         : Open AmbaBrowser (HTML5 Web Browser + Search + Bookmarks)
- browse <url/q>  : Open an HTTPS URL or search the web in AmbaBrowser
- doom            : Launch 3D FPS game AmbaDoom (WASD/Arrows move, Space/Click fire, M radar, R restart)
