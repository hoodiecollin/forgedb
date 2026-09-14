The socket read and write timeout [`CoordinatorClient::connect`] uses: 35 seconds.

It is also the deadline the client declares on every `RequestTurn`, which the coordinator clamps its grant wait to fit inside; a coordinator that receives no declaration assumes this same value.
