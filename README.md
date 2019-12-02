An address-based spam tree, based on the idea that a signup from a region of the address space containing mostly confirmed spam is worth flagging.

The tree should be persistent in general, but it’s okay if some writes are lost.

The address length and the number of bits per level are set at compile time. By default, the length is 16 bytes to fit IPv6 (with IPv4 in ::ffff:0:0/96), and each node represents 4 bits.

As a compromise between accuracy and data collection, trusted addresses are only stored with enough precision to distinguish them from spam.

The current implementation uses a basic pointer-per-node structure and always stores full spam addresses for simplicity, but there are more efficient and DoS-resistant options if this service becomes the weakest link.

Recency and patterns in time aren’t given any weight (or stored at all) yet.


## Protocol

A request’s type is identified by its first byte.

- [0, *address*×*address-bytes*]

	Requests information about an address. The response is [*trusted*×4, *spam*×4, *bits*], where *bits* is the size of the prefix used to determine the result, *trusted* is the number of trusted hits with that prefix, and *spam* is the number of spam hits with that prefix. All values are big-endian and unsigned.

- [1, *address*×*address-bytes*]

	Marks an address as trusted. The response is [0] for success, [1] for failure.

- [2, *address*×*address-bytes*]

	Marks an address as spam. The response is [0] for success, [1] for failure.

It’s okay to send multiple requests without waiting for a response; the responses will come back in order.
