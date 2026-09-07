# Security

Virga parses JSON it did not write, from Open-Meteo, asks GitHub which
release is newest, writes files into your config directory, and ships an
install script that people pipe into `sh`. Those are the places a report is
most likely to be about.

## Reporting a vulnerability

Report it privately through a
[GitHub security advisory](https://github.com/t-shahan/virga/security/advisories/new),
not as a public issue, so a fix can ship before the details do.

This is a side project with one maintainer. Expect an acknowledgement within
a week and a fix or a considered answer within a month; a report that shows
how to make the app run something it should not gets attention sooner than
that.

## Supported versions

Fixes go into the newest release only. There is no long-term branch.
`virga update` says whether a newer release exists and how your install
gets it; the install script, `brew upgrade`, or a fresh download is the
upgrade.

## What counts

Anything that makes Virga do something the user did not ask for: a response
from a weather host that crashes or hangs the app, a path escaping the config
directory, a file written unsafely, a way for the install script to leave a
bad binary on `PATH` or to run something it downloaded without checking it.
The requirements the code is reviewed against are in
[`CLAUDE.md`](CLAUDE.md), under "Security".
