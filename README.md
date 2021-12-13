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
### Start a new httpbin service on Bootstrap Node

```
docker run -it --rm -p 8923:80 kennethreitz/httpbin
```
### Register a new service on Bootstrap Node

```bash
curl --location --request POST 'http://127.0.0.1:8082/service' \
--header 'Content-Type: application/json' \
--data-raw '{
    "peer_id": "",
    "id" : "httpbin_service",
    "domain_name": "localhost",
     "is_deleted" : false,
    "providers": [
        {
            "id" : "service_provider1",
            "name": "httpbin_service_provider1",
            "desc": "httpbin service provider1",
            "base_url": "localhost:8923",
            "schema": "http"
        }
    ]
}'
```

The new service will be forward to the whole p2p network. So you can query it from client node. 


### Query new service from Client Node

```bash
curl --location --request GET 'http://127.0.0.1:8084/service'
```

### Query other service from Client Node
```bash
curl --location --request GET 'http://127.0.0.1:8084/local'
curl --location --request GET 'http://127.0.0.1:8084/remote'
curl --location --request GET 'http://127.0.0.1:8084/peers'
```

### Test forward service
#### Http
```bash
curl http://127.0.0.1:8086/v1/testkey/anything/foobar
```
The request is sent following this flow. User-->Client node-->Bootstrap node-->Service Provider. 

The response is sent following this flow. Service Provider-->Bootstrap node-->Client node-->User.