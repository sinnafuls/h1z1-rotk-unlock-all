# h1z1-rotk-unlock-all

An in-process cosmetics changer for H1Z1 (the ROTK / Z1 build). Every skin
in the game's own cosmetics screen is selectable, a click applies it, the
choice persists across sessions and shows on the lobby character and the
in-match model.

Client-side only: nothing is sent to the server, and other players see the
server's view of you. There is no client-side way around that on a server
that keys cosmetics on owned item instances.

## Use

1. Download `unlock-all.zip` from the latest release and extract it anywhere.
2. Start the game and wait for the lobby.
3. Run `injector.exe`. It loads `unlock_all.dll` into `H1Z1.exe` and stays
   open as a console showing what the payload does.
4. Open the cosmetics screen: every tile is unlocked. Click one to apply it.

Choices are saved to `unlock_all_skins.txt` next to the DLL and restored on
the next session. `unlock_all.log` next to it holds the full trace.

Running `injector.exe` again while the game is up only attaches the console.
The console accepts commands: `run held`, `run skins`, `run populate`,
`u32 <hex>`, `u64 <hex>`, `read <hex> <len>`, `ping`, `quit`.

## Build

```
cargo build --release
```

produces `target/release/injector.exe` and `target/release/unlock_all.dll`.
Keep the two files together.

## Supported build

The payload targets one game build. Every hook verifies the bytes at its
site before installing; after a game update the console reports which site
moved and nothing is installed. The addresses live in
`payload/src/offsets.rs`.

## How it works

The game renders an item from its definition id, and a skinned item is
another definition of the same family. A choice is applied in three places:

- **The skin collection.** The lobby character is dressed from it. A click
  writes the collection entry the way the server's `SetSkinItem` packet
  does, then runs the same refreshers, so the character re-dresses at once.
  Every server rebuild of the collection (lobby return, match join) is
  followed by the stored choices being applied again.
- **The definition lookup.** A prototype with a choice resolves to the
  chosen output definition, so the stowed model, HUD and inventory agree
  with the in-match draw.
- **The local player's attachments.** The server's equipment packet no
  longer replaces the client-built attachments for the local player.

The cosmetics grid decides the lock badge by joining every prototype x skin
pair against the account-item data source, which holds one row per item the
account owns. After the game populates that source, a row per skin
definition is added through the game's own row adder, with the count and
owned columns answering 1.

## Layout

| path | contents |
|---|---|
| `injector/` | the loader and console |
| `payload/src/skins.rs` | the changer: the click, the collection write, the definition lookup, the draw |
| `payload/src/rows.rs` | the lock badge: the account-item rows |
| `payload/src/gamelog.rs` | the game's own log mirrored into the console |
| `payload/src/console.rs` | the command port |
| `payload/src/store.rs` | the persisted choices |
| `payload/src/offsets.rs` | every address, with the bytes each hook site must have |

## Licence

MIT.
