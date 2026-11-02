# OctoWoW MPQ duplication: findings

Measured on the `Fresh Install\Data` folder (9.5 GB, 18 archives) with a tool that reads every
archive in the client's real mount order, looks every known file name up in every archive, and
compares each hidden copy's content with the copy the game actually reads.

## Short answer

Yes, there is a lot of duplicated data, but it is almost all **old versions** that later patches
replace, not identical copies. It costs **disk space only**: the game never reads a hidden copy,
so it has no effect on loading speed or frame rate.

| | Files | Uncompressed | On disk |
|---|---:|---:|---:|
| Hidden copies identical to the version the game reads | 744 | 63 MB | (part of below) |
| Hidden copies superseded by a newer version | 15,169 | 6,459 MB | (part of below) |
| **All hidden (never read) copies** | **15,913** | **6,522 MB** | **2,975 MB** |

So about **3 GB of the 9.5 GB Data folder (31%) is never read.**

### Where the dead bytes are (on disk)

| Archive | Dead on disk |
|---|---:|
| `terrain.MPQ` | 1,258 MB |
| `patch.MPQ` | 681 MB |
| `patch-3.mpq` | 297 MB |
| `wmo.MPQ` | 258 MB |
| `patch-4.mpq` | 258 MB |
| `model.MPQ` | 115 MB |
| `interface.MPQ` | 66 MB |
| `texture.MPQ` | 37 MB |
| others | < 5 MB |

Also: `base.MPQ` (12.6 MB) and `backup.MPQ` (6.4 MB) are not in the 1.12 client's mount order at
all, so neither the official client nor Benilla reads them. And `patch-1.mpq` and `patch-2.mpq`
being almost the same size is a coincidence: they hold different files (85 and 42 listed).

## What Benilla does about it

Nothing in the install changes, on purpose:

- **No runtime cost to remove.** Benilla resolves each file to the highest-priority archive that
  holds it and reads only that copy, exactly as the official client does. Dead copies are never
  touched, so removing them would not make anything faster.
- **The install is OctoWoW's.** Its launcher checks and patches these archives. Rewriting them would
  make the launcher re-download them or fail its checks, and a future patch that drops a file it
  used to override would then find the older copy gone. Benilla never writes to the install.

## The safe way to reclaim the 3 GB

The only way to reclaim it without breaking future updates is on OctoWoW's side: when they next
publish a full client, build the base archives from the current files instead of shipping the old
ones plus overrides. Benilla's reader already handles any archive layout, so a consolidated client
works with no Benilla change, and future `patch-N.mpq` files keep working because the chain is
discovered from the folder at every launch.

A Benilla-side "compact copy" (a stripped duplicate of the Data folder for Benilla alone) would
only save space if the official client and launcher were no longer used on that machine, and it
would have to be rebuilt from the originals after every OctoWoW update. It is not worth it while
both clients share one install.
