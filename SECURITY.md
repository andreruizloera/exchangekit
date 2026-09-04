# Security policy

exchangekit is a play-money simulator intended to run locally or on a
trusted network. It has no authentication: any client that can reach the
gateway can trade as any account. Do not expose the gateway or frontend
to the public internet as-is, and never put real value behind it.

The gateway does parse untrusted input (JSON request bodies, query
parameters, WebSocket frames), so memory-safety or denial-of-service
issues are still worth reporting.

## Reporting

Report vulnerabilities through GitHub's private vulnerability reporting
on this repository, or open an issue if the problem is not sensitive.
Please include a reproduction. You should get a response within a week.
