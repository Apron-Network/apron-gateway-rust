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
./target/debug/apron-gateway --secret-key-seed 1
```
**OR**
```bash
cargo run -- --secret-key-seed 1
```

### Client Node

```bash
./target/debug/apron-gateway --secret-key-seed 2 --peer /ip4/127.0.0.1/tcp/2145/p2p/<peer id from bootsrap> --p2p-port 2149 --mgmt-port 8084 --forward-port 8086
```

### register a new service on Bootstrap Node

```bash
curl --location --request POST 'http://127.0.0.1:8082/service' \
--header 'Content-Type: application/json' \
--data-raw '{
    "peer_id": "",
    "id" : "test",
    "domain_name": "localhost",
     "is_deleted" : false,
    "providers": [
        {
            "id" : "test_provider1",
            "name": "test_provider1 http provider1",
            "desc": "test http provider1 desc",
            "base_url": "localhost:8080",
            "schema": "ws"
        },
        {
            "id" : "test_provider2",
            "name": "test_provider1 http provider1",
            "desc": "test http provider1 desc",
            "base_url": "localhost:8080",
            "schema": "ws"
        }

    ]
}'
```

The new service will be forward to the whole p2p network. So you can query it from client node. 


### Query new service from Client Node

```bash
curl --location --request GET 'http://127.0.0.1:8086/service'
```

### Query other service from Client Node
```bash
curl --location --request GET 'http://127.0.0.1:8086/local'
curl --location --request GET 'http://127.0.0.1:8086/remote'
curl --location --request GET 'http://127.0.0.1:8086/peers'
```

### Test forward service
#### One direction Requst -> Client -> Bootstrap
```bash
curl http://127.0.0.1:8086/v1/anything
```