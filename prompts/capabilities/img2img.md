The change request names an existing image in the workspace and how to transform it. This
capability is a stub: there is no real image-generation backend wired in yet, and no dependency on
the `ks-llm-image` crate (`apps/kosmos/libs/ks-llm-image`) — that crate exists independently and is
not imported here.

Locate the source image the change request refers to, but do not call out to any image-generation
API. Instead, write a small placeholder artifact (e.g. a text file describing the requested
transformation) at a path that fits the workspace's conventions, and say clearly in the summary
that this is a stub with no real image behind it.
