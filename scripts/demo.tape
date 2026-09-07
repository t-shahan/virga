# The README demo, as a script rather than a screen recording, so it can be
# re-cut after a UI change without anyone reaching for a capture tool.
#
#     cargo build --release && vhs scripts/demo.tape
#
# vhs (https://github.com/charmbracelet/vhs) renders a real terminal in a
# headless browser and writes demo.mp4 into the repository root. GitHub only
# plays a video that was dragged into its editor, so the finished file still
# has to be uploaded by hand to become a user-attachments URL for README.md.
#
# The recording runs against a throwaway HOME so it never reads or writes
# your own state file, and with location detection off so it opens on the
# built-in default city rather than wherever the machine happens to be.

Output demo.mp4

Set Shell bash
Set FontFamily "JetBrainsMono Nerd Font"
Set FontSize 15
Set Width 1400
Set Height 900
Set Padding 18
Set WindowBar Colorful
Set WindowBarSize 36
Set Theme "Catppuccin Mocha"
Set Framerate 30
Set TypingSpeed 60ms

Hide
Type "export HOME=$(mktemp -d) PATH=$PWD/target/release:$PATH VIRGA_GEOIP=off VIRGA_UPDATE=off VIRGA_CACHE=off"
Enter
Type "clear"
Enter
Show

# Launch and land on the current conditions screen.
Type "virga"
Sleep 600ms
Enter
Sleep 3.5s

# Step through the coming days, then jump back to now.
Right
Sleep 700ms
Right
Sleep 700ms
Right
Sleep 700ms
Right
Sleep 1s
Type "n"
Sleep 1.2s

# The hourly weathergram: hours along the axis, then a day at a time.
Type "p"
Sleep 3s
Right@250ms 8
Sleep 1s
Down
Sleep 900ms
Down
Sleep 1.2s
Type "n"
Sleep 1s

# The classic precipitation view is one key away, and one key back.
Type "v"
Sleep 2.2s
Type "v"
Sleep 1s
Type "b"
Sleep 1.2s

# Seven themes, round trip; the bar names each one as it lands.
Type "t"
Sleep 1s
Type "t"
Sleep 1s
Type "t"
Sleep 1s
Type "t"
Sleep 1s
Type "t"
Sleep 1s
Type "t"
Sleep 1s
Type "t"
Sleep 1s

# Metric and back.
Type "u"
Sleep 1.6s
Type "u"
Sleep 1s

# Search for a city: Enter runs the search, Enter again picks the top hit.
Type "l"
Sleep 800ms
Type "Tokyo"
Sleep 1.2s
Enter
Sleep 2.5s
Enter
Sleep 3.5s
Type "p"
Sleep 3s
Type "b"
Sleep 1s

# Every key for the screen, on a card.
Type "?"
Sleep 2.5s
Escape
Sleep 1s

# Quit, then the same forecast as one line for a script or a status bar.
Type "q"
Sleep 1s
Type "virga now"
Sleep 400ms
Enter
Sleep 4s
