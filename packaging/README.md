# Packaging

Ways to install freenapse other than downloading `freenapse.exe` from the
releases page. Both sidestep the SmartScreen "Windows protected your PC" prompt
that an unsigned download gets on first run, because neither goes through the
browser's download path.

## winget

`winget/` holds the three manifests of the `MatteoSantoro.freenapse` package
(version, installer, default locale). The executable is a `portable`
installer: winget copies it into its portable packages folder and adds the
`freenapse` command to the PATH, with no installer and no registry entries.

Publishing a version is a pull request to
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) that adds
the three files under `manifests/m/MatteoSantoro/freenapse/<version>/`. The
first submission is reviewed by a person; later ones are mostly automatic.
The quickest way is the `wingetcreate` tool, which fills the hash from the
release asset and opens the pull request:

```
winget install Microsoft.WingetCreate
wingetcreate update MatteoSantoro.freenapse --version 0.3.0 --urls https://github.com/MatthewSaintBull/freenapse/releases/download/v0.3.0/freenapse.exe --submit
```

Before that first submission, `winget validate --manifest packaging/winget`
checks the files, and `winget install --manifest packaging/winget` installs
from them locally (needs `winget settings --enable LocalManifestFiles` once).

Once the package is in, users run:

```
winget install MatteoSantoro.freenapse
```

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

## On a new release

The installer manifest and the Scoop manifest carry the version, the download
URL and the SHA-256 of `freenapse.exe`; `wingetcreate update` and Scoop's
`checkver` fill them in. If you edit by hand, the hash is the content of
`freenapse.exe.sha256` on the release (uppercase for winget).
