# Capture to citation

Construct a checked `WebCapture` from already-retrieved bytes, decode it into a
checked `WebRepresentation`, and select Unicode scalar offsets. Then create an
`EvidenceAnchor`, persist capture, representation, and anchor in that order,
and reload it before rendering. Reload recomputes both content identities and
revalidates the selector.

This recipe performs no network operation and accepts no provider snippet.
