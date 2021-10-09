# The Rust P2P Gateway for Apron Project

## Build

```bash
cargo build
```

## Environment setup

### Standalone

#### Bootstrap Node
The bootstrap node can be started with this command.

```bash
./target/debug/apron-gateway
```
### Client Node

```bash
./target/debug/apron-gateway --peer /ip4/127.0.0.1/tcp/2145 --p2p-port 2146 --mgmt-port 8083
```

### register a new service on Bootstrap Node

```bash
curl --location --request POST 'http://127.0.0.1:8082/service'
```

The new service will be forward to the whole p2p network. So you can query it from client node. 


### Query new service from Client Node

```bash
curl --location --request GET 'http://127.0.0.1:8083/service'
```