An address-based spam tree, based on the idea that a signup from a region of the address space containing confirmed spam is worth flagging.

The tree should be persistent in general, but it’s okay if some writes are lost.

The address length is set at compile time. By default, the length is 16 bytes to fit IPv6 (with IPv4 in ::ffff:0:0/96).


## Use

Enqueue a trust message:

- when a user is marked as trusted
- when a trusted user authenticates
- randomly when a trusted user makes a request

Enqueue a spam message:

- when a user is marked as spam
- when a spam user authenticates
- randomly when a spam user makes a request

Query:

- when a user signs up

(The user ids provided to iptooled can, and often should, be keyed-hashed or encrypted versions of actual user ids.)


## Protocol

A request’s type is identified by its first byte.

- [0, *address*×*address-bytes*]

    Requests information about an address. The response is [*trusted*×4, *spam*×4, *bits*], where *bits* is the size of the prefix used to determine the result, *trusted* is the number of trusted hits with that prefix, and *spam* is the number of spam hits with that prefix. All values are big-endian and unsigned.

- [1, *address*×*address-bytes*, *user*×*user-bytes*]

    Marks an address as associated with a trusted user. The response is [0] for success, [1] for failure.

- [2, *address*×*address-bytes*, *user*×*user-bytes*]

    Marks an address as associated with a spam user. The response is [0] for success, [1] for failure.

It’s okay to send multiple requests without waiting for a response; the responses will come back in order.
