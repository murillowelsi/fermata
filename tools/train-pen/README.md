# Training the pen model (F-40, F-41, F-42)

The app ships one set of weights. Both are made by the app itself (native build), reproducibly:

```sh
tools/train-pen/fetch-homus.sh                                   # HOMUS into tools/train-pen/data (not in git)
fermata --train-pen-model assets/pen_model.bin --homus tools/train-pen/data/HOMUS   # default: HOMUS + synthetic
fermata --train-pen-model assets/pen_model_synthetic.bin                            # plan B: synthetic only
fermata --eval-pen-model --homus tools/train-pen/data/HOMUS     # accuracy per class, HOMUS writers 81-100 held out
```

- Each file is an ensemble of 4 networks (seeds 7, 1007, 2007, 3007), trained at once, one thread each (about 8
  minutes with HOMUS on an M-series Mac). Weights are stored as 8-bit steps of a per-row scale (`FPM3`, about 210 KB).
- The app averages the networks over the symbol as written and four slightly turned and stretched views of it.
- HOMUS writers 81–100 never enter training: they are the test closest to real handwriting.
- Each HOMUS sample of writers 1–80 is trained on as written and 9 times moved a little (slant, scale, each stroke a
  little off on its own, a gentle bend).
- Plan B: `cargo build --release --features pen-synthetic-only` ships `pen_model_synthetic.bin` instead (no HOMUS in
  the weights). One flag, nothing else changes.
- HOMUS declares no licence; it is used for training with the permission of Fermata's maintainer and is not
  redistributed. Credits in CREDITS.md.
