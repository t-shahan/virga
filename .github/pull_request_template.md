## Summary

<!-- What changes, and why. Link the issue if there is one. -->

## Review focus

<!-- Claude reviews every pull request on its own against the requirements in
     CLAUDE.md: security, panics and error handling, readability, test
     coverage, cross-platform input. You do not have to ask for that.

     What it cannot reconstruct from the diff is where the risk actually sits.
     Say that here.

     Mentioning Claude by handle anywhere in this description additionally
     hands the whole pull request to the interactive agent, which answers
     questions and can run the cargo gates. Use it when you want a
     conversation rather than a review. Leaving it out is the normal case.

     The handle is deliberately not written here: this comment is part of the
     description GitHub sends, so spelling it out would trigger the agent on
     every pull request that keeps the template. -->

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
