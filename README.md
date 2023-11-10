# Quantum Block Device

> Note on the name the `quantum` part is planned for the future when the device start to persist the content of the device
across multiple locations with erasure coding so they become quantum safe.

The goal behind building this was the following:

- [x] Allow caching of hot pages on fast SSD storage
- [x] Persist colder pages on a slower storage `HDD`
  - [ ] Also store on remote storage
- [x] Support multiple persisted storage segments, which allows creating a device bigger than a single HDD available
- [x] Pre-allocation of cache and storage segments, which grantees writes will never fail (unless underlying device is broken)
  - This also allows fast retrieval of pages
- [ ] Erasure coding

## Building

To build qbd make sure you have rust installed then run the following commands:

```bash
# this is needed to be run once to make sure the musl target is installed
rustup target add x86_64-unknown-linux-musl

# build the binary
cargo build  --release --target=x86_64-unknown-linux-musl
```

the binary will be available under `./target/x86_64-unknown-linux-musl/release/qbd` you can copy that binary then to `/usr/bin/`
to be able to use from anywhere on your system.

## Usage

```bash
qbd --help
block device in user space

Usage: qbd [OPTIONS] --nbd <NBD> --cache <CACHE> --store <STORE>

Options:
  -n, --nbd <NBD>                path to nbd device to attach to
  -c, --cache <CACHE>            path to the cache file, usually should reside on SSD storage
      --cache-size <CACHE_SIZE>  cache size has to be multiple of page-size [default: "10.0 GiB"]
      --page-size <PAGE_SIZE>    page size used for both cache and storage [default: "1.0 MiB"]
      --store <STORE>            url to backend store as `file:///path/to/file?size=SIZE` accepts multiple stores, the total size of the disk is the total size of all stores provided
  -m, --metrics <METRICS>        listen address for metrics. metrics will be available at /metrics [default: 127.0.0.1:9000]
      --disable-metrics          disable metrics server
      --debug...                 enable debugging logs
  -h, --help                     Print help
  -V, --version                  Print version
```

To setup the cache, the `cache-size` (also store size) **MUST** be multiple of `page-size`. The `page-size` is by default `1mib``. This is the size of one commit operation to storage, so if it's too small, there will be many commits on cache eviction, too big will be fewer but bigger write transactions.

The `store` can be provided multiple times on the command line. All stores will be `concatenated` and act as one. This allows you to have an `nbd` device that is bigger than any local `hdd` device alone.

Write now only `file` store is support, it's `URL` must be as follows:

```bash
--store "file:///path/to/file?size=<SIZE>"
```

- `/path/to/file` is absolute path to the storage file that will be used.
- `SIZE` is required `url` param and can be number of bytes, or any valid size value (for example `100gib` for 100 gigabytes)
- when provided multiple stores, the total size of the block device is the total size of all provided stores.

Note that the `cache-size` **DOES NOT** add to the full size of the `nbd` device. Only the total size of provided stores are! the cache works as `WOL` (write ahead log) in the sense that it's part of the database (deleting the cache will cause possible loss of data).

On cache eviction (when there is no space left in cache) the least used pages will finally evicted to storage (provided by `store` flag)

## Example

To be able to attach to `nbd` you need root privileges with `sudo`

```bash
sudo qbd -n /dev/nbd0 --cache-size 20gib --cache /opt/disk.cache --store "file:///mnt/disk0/disk.sig0?size=100gib"  --store "file:///mnt/disk1/disk.sig1?size=100gib"
```

This will start the service for `/dev/nbd0`, the size of the device will be `200 GiB` (split over 2 segments each of `100 GiB`). The device will have a `20 GiB` cache under `/opt/disk.cache`

## IMPORTANT

- After the first first creation sizes should never be changed. Changing `cache-size`, or `page-size` will render the entire cache invalid which will cause loss of data. Same applies to `store` size. But it's possible to extend the device by adding extra store.
- The order of how the `--store` are provided matters. in the example above `disk.sig0` comes before `disk.sig1` changing the order in a next run will again cause data loss
- We will add protection against those kind of mistakes later on
