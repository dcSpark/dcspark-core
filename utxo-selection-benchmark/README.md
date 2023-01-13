# UTxO selection benchmark

Along with utxo selection library we provide utxo selection algorithms benchmarking library.
This library includes: 
* carp events fetcher & filter -- the tool that takes events from carp database, so the algorithms can be benchmarked using them
* benchmarking tool itself

This library can be used to compare the algorithms: how they behave, how they affect the fees, what will be the final utxo sets and so on.

Currently, the library supports only events with inputs balance = outputs balance + fee.

