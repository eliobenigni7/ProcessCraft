# ProcessCraft app icon and identity

A **sheet of process paper**: a slightly tilted page with a pool header band down its left edge,
carrying a small BPMN process — a start event, a task, an exclusive gateway with a rework loop back
to the task, and an end event below. It reads as "a process on paper" from 1024 px down to 32 px.

The icon follows the tile conventions of the open-source Crafting Apps family (FigCraft, GridCraft,
WordCraft, …) so ProcessCraft sits naturally next to them: a full-bleed app-colour tile, a
three-colour palette and thick ink line work. The artwork itself is original to ProcessCraft.

## Palette

Exactly three colours:

| Colour | Hex | Used for |
|---|---|---|
| Ink | `#0b0b0c` | line work, arrows, the end event |
| Paper | `#efe9dc` | the page, start event and task |
| ProcessCraft amber (app colour) | `#f2a516` | the tile, the pool header band, the gateway |

For text on light backgrounds (badges, links) use the darker **amber ink** `#9a6400`, which keeps
WCAG AA contrast against white; `#f2a516` is reserved for fills.

## Geometry

A 512-unit tile (`viewBox="0 0 512 512"`), rounded square with `rx=112`, no border. The page is
rotated −6° around the tile centre. Windows and Linux use the tile edge to edge.

## Files

- `processcraft.svg`: canonical vector master.
- `processcraft-<size>.png` (16–1024): raster renders; the 256 px one is the runtime window icon
  (`apps/processcraft/src/widgets.rs`) and the README header image.
- `processcraft.ico`: Windows icon, 16–256 px.

## Regenerate

All files come from one geometry in [`scripts/generate_app_icon.py`](../../scripts/generate_app_icon.py)
(requires Pillow):

```sh
python scripts/generate_app_icon.py
```

## License

Original artwork made for ProcessCraft, licensed like the rest of the project under the
[MIT License](../../LICENSE). It contains no third-party marks or artwork.
