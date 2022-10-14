<img align="right" width="150" height="150" top="100" src="./assets/subway.png">

# subway-rs • [![ci](https://github.com/abigger87/subway-rs/actions/workflows/ci.yaml/badge.svg?label=ci)](https://github.com/abigger87/subway-rs/actions/workflows/ci.yaml) ![license](https://img.shields.io/badge/License-MIT-green.svg?label=license) ![twitter](https://img.shields.io/twitter/follow/asnared?style=social)

Construct evm-based sandwich attacks using Rust and Huff.


### What is this?

[subway-rs](https://github.com/abigger87/subway-rs) is a port of [libevm](https://twitter.com/libevm)'s original [subway](https://github.com/libevm/subway), implemented with [ethers-rs](https://github.com/gakonst/ethers-rs) and [huff](https://github.com/huff-language).

> Having highly optimized contracts is just one part of the equation, a tech stack is just as important as the contracts to execute on the opportunities.
_Source: [libevm/subway](https://github.com/libevm/subway#subway)_

If having a tech stack is just as important as optimized, secure contracts, then why not use the best language available for speed, dependability, and scalability? This is Hugo: A pure-rust bot used to execute sandwich attacks on UniswapV2. Hugo's goal is to act as a low barrier of entry for rust-based MEV development - Reference source code for aspiring new searchers.

Hugo is **fast**. But don't take our word for it, just check out the [benchmarks](./hugo/benches).

Alongside [Hugo](./hugo/), we have published [mev-rs](https://crate.io/crates/mev-rs) (the library located in [./mev-rs](./mev-rs/src/lib.rs)): generalized, modular rust infrastructure that you are encouraged to improve upon and extend for your own MEV operations!


Hugo is able to:

- Watch pending transactions in the mempool.
- Decode Transaction data for Uniswap V2 Calls. (and more soon!)
- Verbose telemetry reporting using [tracing](https://crates.io/crates/tracing).
- Search for profitable strategies efficiently using a variety of algorithms.
- Calculate gas bribes.
- Simulate and Execute Flashbot Bundles.


### Future Improvements for Hugo

Although the bot functions, it is simplistic and _certainly_ not competitive. Accept that advanced searchers to already be executing far beyond Hugo's capabilities. That being said, below is a non-exhaustive list of low hanging fruit to further Hugo + subway-rs along.

- [ ] Deployment logic for contracts
- [ ] Circuit Breakers
- [ ] Alerting (see: https://github.com/DeGatchi/mev-template-rs)
- [ ] Poision Token Filtering
- [ ] Caching + Memoization
- [ ] Enhanced Logging Filters
- [ ] Zero-cost Gas Savings :eyes:
- [ ] Real-time Benchmarking
- [ ] Improved Parallelization
- [ ] Persistent Execution Storage and Tracking (eg: psql, a rekt threshold, P&L)
- [ ] Refactored Wallet Management

Again, please be aware, this bot is intended as a piece of educational content, and not for production use. It has not been run in production, and should not be used in such an environment.


### Blueprint

```ml
.
├─ cabret — A verbose wallet inspector.
|  └─ ...
├─ hugo — A highly optimized, pure rust sandwich bot.
|  └─ ...
├─ mev-rs — Modern and maximally-minimal rust tooling for MEV.
|  └─ ...
└─ contracts — UniswapV2 sandwich attack contracts.
   └─ ...
```


### Acknowledgements

- [subway](https://github.com/libevm/subway)
- [quay](https://github.com/Alcibiades-Capital/quay)
- [mev-template-rs](https://github.com/DeGatchi/mev-template-rs)
- [foundry](https://github.com/foundry-rs/foundry)
- [forge-std](https://github.com/brockelmore/forge-std)
- [foundry-huff](https://github.com/foundry-rs/foundry-huff)


### Contributing

All contributions are welcome!

Please reach out to [asnared](https://twitter.com/asnared) on twitter for any questions or [open an issue](https://github.com/abigger87/subway-rs/issues/new).