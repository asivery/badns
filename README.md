# baDNS

baDNS is a DNS server written in Rust, leveraging the `rustdns` library for core functionality. It uses QuickJS to evaluate configuration files, allowing for dynamic and programmable DNS handling. When a DNS request is received, baDNS first routes the request to the JavaScript context defined by the configuration. If a response is generated within the JS context, it is returned. Otherwise, baDNS queries the configured upstream servers sequentially and caches the response for the specified TTL.

## Features

- **Programmable DNS Handling**: Define custom DNS logic using JavaScript.
- **Flexible Configuration**: Use JavaScript to define how DNS requests are processed.
- **Upstream Server Support**: Automatically query upstream servers if the JS context does not provide a response.
- **Response Caching**: Cache upstream responses based on their TTL.
- **HTTP Reverse Proxy**: Set up HTTP reverse proxy for domains.

## Installation

To install baDNS, follow these steps:

1. Ensure you have Rust installed. If not, download it from [rust-lang.org](https://www.rust-lang.org/).
2. Clone the baDNS repository:
   ```sh
   git clone https://github.com/asivery/badns.git
   cd badns
   ```
3. Build the project using Cargo:
   ```sh
   cargo build --release
   ```
4. The binary will be located in `target/release/badns`.

## Usage

To run baDNS, execute the following command:
```sh
./badns <config-file>
```
Replace `<config-file>` with the path to your JavaScript configuration file.

## Configuration

The configuration file is a JavaScript file evaluated by QuickJS. Below is a summary of the available functions and their purposes:

### Main Initialization

The main baDNS initialization file is executed before the configuration file. It prepares the environment and exposes user-friendly methods for DNS handling.

### Functions

#### Network Setup
- **`bindAddress(address: string, port = 53)`**: Binds the UDP address and starts listening. Multiple interfaces can be open simultaneously.
- **`upstream(address: string, port = 53)`**: Adds an upstream server. Queries upstream servers sequentially if no JS context response is found.

#### HTTP Reverse Proxy
- **`setupHTTPRedirectServer(address: string, port: number, recordTarget: string)`**: Sets up the HTTP reverse proxy server - it needs to know its own IP address, so that it redirects correctly.
- **`addHTTPRedirect(target: string, name: string)`**: Adds a reverse proxy entry to bind the domain `name` to the HTTP address `target`.

#### DNS Bindings
- **`addBinding(rrtype: RRConstant, name: string, handler: Handler)`**: Adds a binding for a specific rrtype and name.
- **`addABinding(name: string, handler: Handler)`**: Adds an RR_A binding.
- **`addAAAABinding(name: string, handler: Handler)`**: Adds an RR_AAAA binding.
- **`addCNAMEBinding(name: string, handler: Handler)`**: Adds an RR_CNAME binding.
- **`addUniversalBinding(handler: Handler)`**: Adds a universal binding triggered on every query unless overridden by specific bindings.

#### Helper Functions
- **`STUB()`**: Returns an RR_A response with an infinite TTL pointing to 0.0.0.0.
- **`permanentBinding(ip: string, domain: string)`**: Adds a permanent RR_A binding.
- **`ban(domain: string)`**: Bans a domain using a STUB() handler.
- **`exec(filename: string)`**: Evaluates the contents of the provided file.

### baDNS Extensions

- **`sha256(data: string)`**: Generates a SHA256 digest of the provided data.
- **`readFile(filename: string)`**: Reads and returns the contents of the specified file as UTF-8.

## Example Configuration

Below is an example configuration file demonstrating basic usage:

```javascript
// Bind to port 53 on all interfaces
bindAddress('0.0.0.0');

// Set up an upstream DNS server
upstream('8.8.8.8');

// Add a permanent A record binding
permanentBinding('192.168.1.1', 'example.com');

// Ban a specific domain
ban('malicious.com');

// Set up the HTTP reverse proxy server on port 80, with the server returning its own address as 192.168.1.2
setupHTTPRedirectServer('0.0.0.0', 80, '192.168.1.2');

// Add a reverse proxy entry to bind the domain 'web.example.com' to HTTP address 'http://localhost:3000'
addHTTPRedirect('192.168.1.123:3000', 'internal.site');

// Universal binding for all queries
addUniversalBinding((name, rrtype, rrclass, peerAddress, ownAddress) => {
    console.log(`Query received: ${name}, type: ${rrtype}`);
    return { special: true, specialType: 'queryUpstream' };
});
```

---

Feel free to reach out with any questions or feedback regarding baDNS. Happy DNS serving!
