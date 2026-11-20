# Training the pen model (F-40, F-41)

The app ships one set of weights. Both are made by the app itself (native build), reproducibly:

```sh
tools/train-pen/fetch-homus.sh                                   # HOMUS into tools/train-pen/data (not in git)
fermata --train-pen-model assets/pen_model.bin --homus tools/train-pen/data/HOMUS   # default: HOMUS + synthetic
fermata --train-pen-model assets/pen_model_synthetic.bin                            # plan B: synthetic only
fermata --eval-pen-model --homus tools/train-pen/data/HOMUS     # accuracy per class, HOMUS writers 81-100 held out
```

- HOMUS writers 81–100 never enter training: they are the test closest to real handwriting.
- Plan B: `cargo build --release --features pen-synthetic-only` ships `pen_model_synthetic.bin` instead (no HOMUS in
  the weights). One flag, nothing else changes.
- HOMUS declares no licence; it is used for training with the permission of Fermata's maintainer and is not
  redistributed. Credits in CREDITS.md.
