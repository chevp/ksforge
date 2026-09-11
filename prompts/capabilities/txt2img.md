The change request below describes what image to generate. This capability is a stub: there is no
real image-generation backend wired in yet, and no dependency on the `ks-llm-txt2img` crate
(`apps/kosmos/libs/ks-llm-txt2img`) — that crate exists independently and is not imported here.

Do not call out to any image-generation API. Instead, write a small placeholder artifact (e.g. a
text file describing the requested image) at a path that fits the workspace's conventions, and
say clearly in the summary that this is a stub with no real image behind it.
