# Packaging

A way to install freenapse other than downloading `freenapse.exe` from the
releases page. It sidesteps the SmartScreen "Windows protected your PC" prompt
that an unsigned download gets on first run, because it does not go through
the browser's download path.

## Scoop

`scoop/freenapse.json` is a Scoop manifest. Scoop installs it straight from a
URL, so it works without a bucket:

```
scoop install https://raw.githubusercontent.com/MatthewSaintBull/freenapse/main/packaging/scoop/freenapse.json
```

Its `checkver` and `autoupdate` sections let a bucket pick up new releases on
its own (the hash comes from the release's `.sha256` file). For `scoop install
freenapse` with no URL, the manifest has to be in a bucket: either the
community `extras` bucket (a pull request to ScoopInstaller/Extras) or a
bucket of this project's own.

freenapse is not on winget. Manifests for it were drafted once and removed
unsubmitted; a future submission starts from the winget-pkgs documentation.

## On a new release

The Scoop manifest carries the version, the download URL and the SHA-256 of
`freenapse.exe`; Scoop's `checkver` fills them in. If you edit by hand, the
hash is the content of `freenapse.exe.sha256` on the release.
