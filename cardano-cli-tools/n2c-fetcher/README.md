# Oura block fetcher

The tool is needed to fetch Cardano blocks from oura with raw cbor. Example:
```shell
 cargo run --bin oura-block-fetcher  -- --bearer tcp --since 94542168,9a585e847251b8e1eb41130c53506f3a5ef60213478af4b42b4477f884f86a59 --socket relays-new.cardano-mainnet.iohk.io:3001
```
# Configuration
There are multiple parameters:
* `bearer -- either tcp or unix` - represents the connection type to the node (tcp web / unix socket)
* `since -- slot,block_hash` - starting point
* `socket -- relays-new.cardano-mainnet.iohk.io:3001` - url or path to cardano node unix socket
* `network -- mainnet / testnet / preview / preprod` - network of interest

