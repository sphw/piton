# Piton

Piton is an experimental IDL (interface definition language). It's designed to be used primarily with zero-copy capable transports, such as a DMA interface on a microcontroller, or io_uring on Linux. Right now it only supports Rust and uses `repr(c)` as the wire format. Support for more languages and wire formats is coming.

## Why

Have you ever wanted to use gRPC over a transport other than HTTP2, and with something lighter weight than protobufs? If so Piton is for you. I've spent a surprisingly large amount of time over the last few years writing small RPC systems over and over again. I wrote one for Bluetooth, one for AWS Nitro Enclave's VSocks, one more for my RTOSes message passing layer, and many more than I can list here. All of these systems were broadly similar, but in the end, tightly coupled to the underlying transport. Worse yet, what if I wanted to switch Bluetooth out for a different protocol, I might have to rewrite the entire framework. Instead, I want a system where I can write code that is independent of the particular transport. Similar to gRPC I even want to be able to use multiple languages. Piton solves this by generating code with pluggable transports. 

## Usage

Piton's syntax is largely similar to Rust, with the addition of two new types: `service` and `bus`. A service implements function call or request-reply semantics. Each method has an argument and a return type. A bus implements a send-only system. You can define a set number of messages the bus accepts. You'll notice that generics are supported throughout Piton.

```
struct Test<T> {
 foo: u16,
 bar: T,
 boolean: bool,
 array: [u8; 20]
}

struct Foo {
 foo: Test<u32>,
 bar: Bar<u32>
}

enum Bar<T> {
  Test,
  B(T)
}

service Driver<D> {
   method xyz(Bar<D>) -> Test<D>
}

bus TestBus<D> {
  msg foo(Bar<D>)
}

```


Piton is still a work in progress, check out `example` to get a feel for how to use it. This documentation section will be fleshed out in the future.
