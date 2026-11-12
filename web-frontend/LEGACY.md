# Retired: this is not the UI

Flutter (`apps/flutter/`) is the only user interface. This React application is kept for reference
only: no workflow builds it, no compose stack runs it, and dependabot no longer tracks it.

Migrate a screen when you are about to change it. When the last screen has a Flutter equivalent,
delete this directory; keeping a second UI alive is the thing the architecture is trying to avoid.
