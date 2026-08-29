## Summary

<!-- What changes, and why. Link the issue if there is one. -->

## Review focus for @claude

@claude review this pull request against the requirements in
[CLAUDE.md](https://github.com/t-shahan/virga/blob/main/CLAUDE.md).
Cover security, panics and error handling, readability, test coverage, and
cross-platform input. Report general quality problems, not only outright bugs.

<!-- Then tell it where to look hardest. Delete the line below if the
     standard sweep above is genuinely all this needs. -->

Look hardest at:

## Test plan

<!-- The four gates, plus anything manual. Rendering is not covered by CI. -->

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets --locked -- -D warnings`
- [ ] `cargo test --locked --all-targets`
- [ ] `cargo package --locked`
- [ ] Manual render check, if this touches layout or drawing

## Changelog

<!-- Delete whichever does not apply. -->

- [ ] Described under the topmost section of `CHANGELOG.md`
- [ ] Not user-observable, so it owes no entry
