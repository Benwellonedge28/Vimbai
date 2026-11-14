# Event compatibility policy

The socket, the bus and the records a workflow reacts to are a second API. It is easier to break than the first, because
an event has no request to fail: a client that mishandles an event simply behaves wrongly, later, quietly.

## What an event is here

- The **live socket** (`/websocket`): a client connects, is told it is connected, and then receives a Book's events as
  they happen. A client's write is relayed to the rest of the channel.
- The **bus** (`/message-bus`): the engine's own publish path. A publish is handed to a broker in the background, and each
  pod declares its own queue so one publish reaches every replica rather than one lucky one. A broker that is down costs
  cross-pod reach, never a failed write.
- A **trigger**: the automation and workflow domains react to an entity being created, updated or deleted. That reaction
  is an event contract with a consumer inside this repository rather than a client outside it.

## Compatible

- A new event **type** alongside the existing ones, which a client that does not know it ignores.
- A new field on an existing event, with the same rule as responses: unknown fields are ignored by every sane consumer.
- Additional delivery of an existing event - a second replica receiving it - because each pod has its own queue and
  duplicate delivery across pods is a delivery guarantee, not a duplicate.

## Breaking

- Renaming or removing an event type, or changing its meaning while keeping its name.
- Removing a field a consumer reads, or changing its type. The temptation here is to "tidy" an amount into a number;
  events carry money as strings for the same reason responses do.
- Changing **ordering** guarantees: an event that used to arrive before another arriving after it is a break, even if
  both arrive.
- Changing **who receives** an event. A Book's events go to that Book's sockets; widening or narrowing that set is a
  privacy change as much as a compatibility one.
- Changing the stamp. Events carry an ISO instant; a service once sent seconds in the same field, which is a break that
  no consumer would report as an error and every consumer would misread. The shape of a stamp is part of the contract.

## Delivery semantics a consumer may assume

- An event is delivered to every connected replica of its Book, not to one of them.
- A publish that is accepted is not lost because a broker is unavailable; it may be delayed in reach, not dropped.
- A write on a socket reaches the rest of its channel, and the writer is not sent its own event back. A consumer that
  depends on receiving its own write is depending on something never promised.
