# Third-party notices

OnKey is MIT-licensed (see [LICENSE](LICENSE)). It includes the third-party
models, data, fonts and libraries below. Each is used under the licence named here, and this file carries the
copyright notices and licence texts those licences ask to be reproduced.

This file must also ship inside the app (it does: `tauri.conf.json` bundles it with the models), because the MIT, BSD, ISC, Apache-2.0
and OFL licences ask for their notices to go with binary copies, not only with the source.

## Models

### CREPE pitch tracker (`crepe-tiny.onnx`, `crepe-full-b32.onnx`)

Exported to ONNX by OnKey from torchcrepe's `tiny` and `full` weights, which torchcrepe converted from the
original CREPE models.

- CREPE: <https://github.com/marl/crepe>, MIT License, Copyright (c) 2018 Jong Wook Kim.
- torchcrepe: <https://github.com/maxrmorrison/torchcrepe>, MIT License, Copyright (c) 2020 Max Morrison.
- Paper: Jong Wook Kim, Justin Salamon, Peter Li, Juan Pablo Bello. "CREPE: A Convolutional Representation for
  Pitch Estimation." ICASSP 2018.

Both are under the MIT License text below.

### wav2vec 2.0 base, fine-tuned on 960 h of LibriSpeech (`wav2vec2-base-960h-fp16.onnx`)

Exported to ONNX (16-bit) by OnKey from torchaudio's `WAV2VEC2_ASR_BASE_960H` pipeline, whose weights are
fairseq's. torchaudio: "Originally published by the authors of wav2vec 2.0 under MIT License and redistributed
with the same license."

- fairseq: <https://github.com/facebookresearch/fairseq>, MIT License, Copyright (c) Facebook, Inc. and its
  affiliates.
- torchaudio pipeline: <https://docs.pytorch.org/audio/stable/generated/torchaudio.pipelines.WAV2VEC2_ASR_BASE_960H.html>
- Trained on LibriSpeech (Panayotov et al., 2015, <https://www.openslr.org/12>, CC BY 4.0).
- Paper: Alexei Baevski, Henry Zhou, Abdelrahman Mohamed, Michael Auli. "wav2vec 2.0: A Framework for
  Self-Supervised Learning of Speech Representations." NeurIPS 2020.

Under the MIT License text below.

### GAME note model, small, v1.0.3 (`game/encoder.onnx`, `segmenter.onnx`, `estimator.onnx`, `dur2bd.onnx`, `bd2dur.onnx`)

ONNX files from the GAME v1.0.3 release (`GAME-1.0.3-small-onnx.zip`), unmodified.

- GAME: <https://github.com/openvpi/GAME>, MIT License, Copyright (c) 2026 Team OpenVPI.

Under the MIT License text below.

### UVR MDX-Net Karaoke 2 (`UVR_MDXNET_KARA_2.onnx`)

Lead-vocal separation model by the Ultimate Vocal Remover project, downloaded unmodified from its model
repository (<https://github.com/TRvlvr/model_repo/releases/tag/all_public_uvr_models>).

- Ultimate Vocal Remover: <https://github.com/Anjok07/ultimatevocalremovergui>, by Anjok07 and aufr33 (UVR's
  core developers, who trained UVR's models).
- UVR's README: "The Ultimate Vocal Remover GUI code is MIT-licensed. For all third-party application developers
  who wish to use our models, please honor the MIT license by providing credit to UVR and its developers."
- The model file carries no licence of its own; it is used on the terms UVR's README states above, with the credit
  below.
- MDX-Net architecture by Kuielab and Woosung Choi.

Credit: **vocal separation by the UVR-MDX-NET Karaoke 2 model from Ultimate Vocal Remover (Anjok07, aufr33).**

## Data

### CMU Pronouncing Dictionary (`src-tauri/data/syllables-en.txt`, derived)

Syllable counts derived from the CMU Pronouncing Dictionary, <http://www.speech.cs.cmu.edu/cgi-bin/cmudict>.

```
Copyright (C) 1993-2015 Carnegie Mellon University. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

1. Redistributions of source code must retain the above copyright
   notice, this list of conditions and the following disclaimer.
   The contents of this file are deemed to be source code.

2. Redistributions in binary form must reproduce the above copyright
   notice, this list of conditions and the following disclaimer in
   the documentation and/or other materials provided with the
   distribution.

This work was supported in part by funding from the Defense Advanced
Research Projects Agency, the Office of Naval Research and the National
Science Foundation of the United States of America, and by member
companies of the Carnegie Mellon Sphinx Speech Consortium. We acknowledge
the contributions of many volunteers to the expansion and improvement of
this dictionary.

THIS SOFTWARE IS PROVIDED BY CARNEGIE MELLON UNIVERSITY ``AS IS'' AND
ANY EXPRESSED OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO,
THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL CARNEGIE MELLON UNIVERSITY
NOR ITS EMPLOYEES BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### US English hyphenation patterns (embedded by the `hyphenation` crate, feature `embed_en-us`)

From the hyph-utf8 package, `hyph-en-us.tex`
(<https://github.com/hyphenation/tex-hyphen/blob/master/hyph-utf8/tex/generic/hyph-utf8/patterns/tex/hyph-en-us.tex>):

```
Copyright (C) 1990, 2004, 2005 Gerard D.C. Kuiken.
Copying and distribution of this file, with or without modification, are permitted in any medium without
royalty provided the copyright notice and this notice are preserved.
```

## Fonts

### Geist and Geist Mono (bundled into the app's interface by `next/font`)

<https://github.com/vercel/geist-font>, Copyright (c) 2023 Vercel, in collaboration with basement.studio.
Licensed under the SIL Open Font License, Version 1.1 (full text in the appendix).

## Libraries compiled into the app

### ONNX Runtime 1.22.0 (statically linked through the `ort` crate)

<https://github.com/microsoft/onnxruntime>, MIT License, Copyright (c) Microsoft Corporation. ONNX Runtime
bundles further open-source components (protobuf, abseil, Eigen, flatbuffers, onnx, re2 and others), listed
with their licences in its ThirdPartyNotices:
<https://github.com/microsoft/onnxruntime/blob/v1.22.0/ThirdPartyNotices.txt>.
Prebuilt binaries provided by pyke.io (`ort`, MIT OR Apache-2.0, Copyright (c) 2023-2025 pyke.io).

### DirectML 1.15.4 (Windows only: `DirectML.dll`, beside `onkey.exe`)

Microsoft's DirectML, from its NuGet package `Microsoft.AI.DirectML` 1.15.4, unmodified: runs the learning models
on the GPU. Copyright (c) Microsoft Corporation, distributed under the Microsoft Software License Terms for
DirectML, which are installed beside it as `DirectML-LICENSE.txt` (<https://www.nuget.org/packages/Microsoft.AI.DirectML/1.15.4/License>).

### JavaScript in the app's interface (`ui/`, static export)

| Package | Licence | Copyright |
|---|---|---|
| next 15.5.27 (incl. its client runtime) | MIT | Copyright (c) 2025 Vercel, Inc. |
| react, react-dom 19.1.0, scheduler 0.26.0 | MIT | Copyright (c) Meta Platforms, Inc. and affiliates. |
| @tauri-apps/api 2.5.0 | Apache-2.0 OR MIT (used under MIT) | Copyright (c) 2017 - Present Tauri Apps Contributors |
| @swc/helpers 0.5.15 | Apache-2.0 | the SWC project's authors, <https://github.com/swc-project/swc> |
| styled-jsx 5.1.6 | MIT | Copyright (c) 2016-present Vercel, Inc. |
| client-only 0.0.1 | MIT | the package's authors |
| tslib 2.8.1 | 0BSD | Copyright (c) Microsoft Corporation. |

Build-only tools (not shipped): Tailwind CSS (MIT), lightningcss (MPL-2.0), PostCSS (MIT), TypeScript
(Apache-2.0), caniuse-lite (CC-BY-4.0, browser-support data used at build time), and sharp/libvips
(Apache-2.0 / LGPL-3.0-or-later, optional Next.js image tool, unused because images are unoptimised in the static
export). The Tauri CLI (Apache-2.0 OR MIT) builds the app and isn't shipped.

### Rust crates

Every crate compiled into the macOS app (normal and build dependencies for `aarch64-apple-darwin`, from
`cargo metadata` on `src-tauri/Cargo.lock`). Where a crate offers a choice ("MIT OR Apache-2.0"), OnKey uses it
under MIT; crates under other licences are used under all the licences listed. Licence texts are in the
appendix. The copyright column is taken from each crate's own licence files; where those carry no copyright
line, the crate's repository names its authors.

| Crate | Version | Licence (SPDX) | Copyright (from the crate's licence files) |
|---|---|---|---|
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | Copyright (C) Jonas Schievink <jonasschievink@gmail.com> |
| aho-corasick | 1.1.5 | Unlicense OR MIT | Copyright (c) 2015 Andrew Gallant |
| alloc-no-stdlib | 3.0.0 | BSD-3-Clause | Copyright (c) 2016 Dropbox, Inc. |
| alloc-stdlib | 0.3.0 | BSD-3-Clause | the crate's authors, https://github.com/dropbox/rust-alloc-no-stdlib |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/anyhow |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | Copyright (c) 2018 Josh Stone |
| base64 | 0.21.7 | MIT OR Apache-2.0 | Copyright (c) 2015 Alice Maz |
| base64 | 0.22.1 | MIT OR Apache-2.0 | Copyright (c) 2015 Alice Maz |
| base64 | 0.23.1 | MIT OR Apache-2.0 | Copyright (c) 2025 Alice Maz, Marshall Pierce |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | Copyright (c) 2014 Steve "Sc00bz" Thomas (steve at tobtu dot com); Copyright (c) 2021-2025 The RustCrypto Project Developers |
| bincode | 1.3.3 | MIT | Copyright (c) 2014 Ty Overby |
| bit-set | 0.8.0 | Apache-2.0 OR MIT | Copyright (c) 2023 The Rust Project Developers |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT | Copyright (c) 2023 The Rust Project Developers |
| bitflags | 1.3.2 | MIT/Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | Copyright (c) 2018-2019 The RustCrypto Project Developers |
| block2 | 0.6.2 | MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| brotli | 9.0.0 | BSD-3-Clause AND MIT | Copyright (c) 2016 Dropbox, Inc.; Copyright (c) 2009, 2010, 2013-2016 by the Brotli Authors. |
| brotli-decompressor | 6.0.1 | BSD-3-Clause/MIT | Copyright (c) 2016 Dropbox, Inc. |
| byteorder | 1.5.0 | Unlicense OR MIT | Copyright (c) 2015 Andrew Gallant |
| bytes | 1.12.1 | MIT | Copyright (c) 2018 Carl Lerche |
| camino | 1.2.6 | MIT OR Apache-2.0 | the crate's authors, https://github.com/camino-rs/camino |
| cargo-platform | 0.1.9 | MIT OR Apache-2.0 | the crate's authors, https://github.com/rust-lang/cargo |
| cargo_metadata | 0.19.2 | MIT | the crate's authors, https://github.com/oli-obk/cargo_metadata |
| cargo_toml | 1.0.1 | Apache-2.0 OR MIT | © Kornel Lesiński |
| cc | 1.8.0 | MIT OR Apache-2.0 | Copyright (c) 2014 Alex Crichton |
| cfb | 0.14.0 | MIT | Copyright (c) 2017 Matthew D. Steele |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | Copyright (c) 2014 Alex Crichton |
| chrono | 0.4.45 | MIT OR Apache-2.0 | Copyright (c) 2014, Kang Seonghoon. |
| cookie | 0.18.2 | MIT OR Apache-2.0 | Copyright 2017 Sergio Benitez; Copyright 2014 Alex Chricton; Copyright (c) 2017 Sergio Benitez |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | Copyright (c) 2012-2013 Mozilla Foundation |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | Copyright (c) 2012-2013 Mozilla Foundation |
| core-graphics | 0.25.0 | MIT OR Apache-2.0 | Copyright (c) 2012-2013 Mozilla Foundation |
| core-graphics-types | 0.2.0 | MIT OR Apache-2.0 | Copyright (c) 2012-2013 Mozilla Foundation |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | Copyright (c) 2020-2025 The RustCrypto Project Developers |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 | Copyright (c) 2018 Sam Rijs, Alex Crichton and contributors |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 | Copyright (c) 2019 The Crossbeam Project Developers; COPYRIGHT AND/OR OTHER APPLICABLE LAW. ANY USE OF THE WORK OTHER THAN AS; copyright protection under copyright law or other applicable laws. |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | Copyright (c) 2019 The Crossbeam Project Developers |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | Copyright (c) 2021 RustCrypto Developers |
| cssparser | 0.37.0 | MPL-2.0 | the crate's authors, https://github.com/servo/rust-cssparser |
| cssparser-macros | 0.7.1 | MPL-2.0 | the crate's authors, https://github.com/servo/rust-cssparser |
| ctor | 1.0.13 | Apache-2.0 OR MIT | the crate's authors, https://github.com/mmastrac/linktime |
| darling | 0.21.3 | MIT | Copyright (c) 2017 Ted Driggs |
| darling_core | 0.21.3 | MIT | Copyright (c) 2017 Ted Driggs |
| darling_macro | 0.21.3 | MIT | Copyright (c) 2017 Ted Driggs |
| der | 0.8.2 | Apache-2.0 OR MIT | Copyright (c) 2020-2026 The RustCrypto Project Developers |
| deranged | 0.4.0 | MIT OR Apache-2.0 | Copyright 2024 Jacob Pratt et al.; Copyright (c) 2024 Jacob Pratt et al. |
| derive_more | 2.1.1 | MIT | Copyright (c) 2016 Jelte Fennema |
| derive_more-impl | 2.1.1 | MIT | Copyright (c) 2016 Jelte Fennema |
| digest | 0.10.7 | MIT OR Apache-2.0 | Copyright (c) 2017 Artyom Pavlov |
| dirs | 7.0.0 | MIT OR Apache-2.0 | Copyright (c) 2018-2019 dirs-rs contributors |
| dirs-sys | 0.5.0 | MIT OR Apache-2.0 | Copyright (c) 2018-2019 dirs-rs contributors |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | the crate's authors, https://github.com/yaahc/displaydoc |
| dom_query | 0.28.0 | MIT | Copyright (c) 2023 Mykola Humanov |
| dpi | 0.1.2 | Apache-2.0 AND MIT | Copyright (c) 2018 Jorge Aparicio; copyright:; Copyright © 2005-2020 Rich Felker, et al. |
| dtoa | 1.0.11 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/dtoa |
| dtoa-short | 0.3.5 | MPL-2.0 | the crate's authors, https://github.com/upsuper/dtoa-short |
| dunce | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | the crate's authors, https://gitlab.com/kornelski/dunce |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/dyn-clone |
| embed-resource | 3.0.12 | MIT | Copyright (c) 2017 nabijaczleweli |
| embed_plist | 1.2.2 | MIT OR Apache-2.0 | Copyright (c) 2020 Nikolai Vazquez |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | Copyright (c) 2016--2023 |
| erased-serde | 0.4.10 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/erased-serde |
| errno | 0.3.14 | MIT OR Apache-2.0 | Copyright (c) 2014 Chris Wong |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | the crate's authors, https://github.com/smol-rs/fastrand |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | the crate's authors, https://github.com/image-rs/fdeflate |
| filetime | 0.2.29 | MIT/Apache-2.0 | Copyright (c) 2014 Alex Crichton |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 | Copyright (c) 2014 Alex Crichton |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | Copyright (c) 2014-2026 Alex Crichton |
| fnv | 1.0.7 | Apache-2.0 / MIT | Copyright (c) 2017 Contributors |
| foldhash | 0.2.0 | Zlib | Copyright (c) 2024 Orson Peters |
| foreign-types | 0.5.0 | MIT/Apache-2.0 | Copyright (c) 2017 The foreign-types Developers |
| foreign-types-macros | 0.2.4 | MIT/Apache-2.0 | Copyright (c) 2017 The foreign-types Developers |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 | Copyright (c) 2017 The foreign-types Developers |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | Copyright (c) 2013-2016 The rust-url developers |
| fst | 0.4.7 | Unlicense/MIT | Copyright (c) 2015 Andrew Gallant |
| generic-array | 0.14.7 | MIT | Copyright (c) 2015 Bartłomiej Kamiński |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | Copyright (c) 2018-2024 The rust-random Project Developers; Copyright (c) 2014 The Rust Project Developers |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | Copyright (c) 2018-2025 The rust-random Project Developers; Copyright (c) 2014 The Rust Project Developers |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | Copyright (c) 2018-2026 The rust-random Project Developers; Copyright (c) 2014 The Rust Project Developers |
| glob | 0.3.4 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| hashbrown | 0.12.3 | MIT OR Apache-2.0 | Copyright (c) 2016 Amanieu d'Antras |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | Copyright (c) 2016 Amanieu d'Antras |
| heck | 0.5.0 | MIT OR Apache-2.0 | Copyright (c) 2015 The Rust Project Developers |
| hex | 0.4.3 | MIT OR Apache-2.0 | Copyright (c) 2013-2014 The Rust Project Developers.; Copyright (c) 2015-2020 The rust-hex Developers |
| html5ever | 0.39.0 | MIT OR Apache-2.0 | Copyright (c) 2014 The html5ever Project Developers |
| http | 1.5.0 | MIT OR Apache-2.0 | Copyright 2017 http-rs authors; Copyright (c) 2017 http-rs authors |
| http-range | 0.1.5 | MIT | Copyright (c) 2016 Luka Zakrajšek |
| httparse | 1.10.1 | MIT OR Apache-2.0 | Copyright (c) 2015-2025 Sean McArthur |
| hyphenation | 0.8.4 | Apache-2.0/MIT | Copyright (c) 2016 tapeinosyne |
| hyphenation_commons | 0.8.4 | Apache-2.0/MIT | the crate's authors, https://github.com/tapeinosyne/hyphenation |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | Copyright 2020 Andrew Straw; Copyright (c) 2020 Andrew D. Straw |
| ico | 0.5.0 | MIT | Copyright (c) 2018 Matthew D. Steele |
| icu_collections | 2.3.0 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| icu_locale_core | 2.3.0 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| icu_normalizer | 2.3.0 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| icu_properties | 2.3.0 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| icu_properties_data | 2.3.0 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| icu_provider | 2.3.1 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| ident_case | 1.0.1 | MIT/Apache-2.0 | the crate's authors, https://github.com/TedDriggs/ident_case |
| idna | 1.1.0 | MIT OR Apache-2.0 | Copyright (c) 2013-2025 The rust-url developers |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | Copyright (c) The rust-url developers |
| indexmap | 1.9.3 | Apache-2.0 OR MIT | Copyright (c) 2016--2017 |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | Copyright (c) 2016--2017 |
| infer | 0.22.0 | MIT | Copyright (c) 2019 Bojan |
| itoa | 1.0.18 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/itoa |
| json-patch | 4.2.0 | MIT/Apache-2.0 | Copyright (c) 2017 Ivan Dubrov |
| jsonptr | 0.7.1 | MIT OR Apache-2.0 | Copyright 2024 Chance Dinkins; Copyright (c) 2022 Chance Dinkins |
| keyboard-types | 0.8.3 | MIT OR Apache-2.0 | Copyright (c) 2017 Pyfisch |
| libc | 0.2.190 | MIT OR Apache-2.0 | Copyright (c) The Rust Project Developers |
| litemap | 0.8.3 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | Copyright (c) 2016 The Rust Project Developers |
| log | 0.4.34 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| markup5ever | 0.39.0 | MIT OR Apache-2.0 | Copyright (c) 2014 The html5ever Project Developers |
| matrixmultiply | 0.3.11 | MIT/Apache-2.0 | Copyright (c) 2016 - 2023 Ulrik Sverdrup "bluss"; Copyright (c) 2021 DutchGhost [constparse.rs] |
| memchr | 2.8.3 | Unlicense OR MIT | Copyright (c) 2015 Andrew Gallant |
| mime | 0.3.17 | MIT OR Apache-2.0 | Copyright (c) 2014 Sean McArthur |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | Copyright 2013-2014 RAD Game Tools and Valve Software; Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC; Copyright (c) 2017 Frommi |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | Copyright 2013-2014 RAD Game Tools and Valve Software; Copyright 2010-2014 Rich Geldreich and Tenacious Software LLC; Copyright (c) 2017 Frommi |
| mio | 1.2.4 | MIT | Copyright (c) 2014 Carl Lerche and other MIO contributors |
| muda | 0.20.0 | Apache-2.0 OR MIT | Copyright (c) 2022-2022 Tauri Programme within The Commons Conservancy |
| native-tls | 0.2.18 | MIT OR Apache-2.0 | Copyright (c) 2016 The rust-native-tls Developers |
| ndarray | 0.16.1 | MIT OR Apache-2.0 | Copyright (c) 2015 - 2021 Ulrik Sverdrup "bluss", |
| new_debug_unreachable | 1.0.6 | MIT | Copyright (c) 2015 Jonathan Reem |
| num-complex | 0.4.6 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| num-conv | 0.1.0 | MIT OR Apache-2.0 | Copyright 2023 Jacob Pratt; Copyright (c) 2023 Jacob Pratt |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| objc2 | 0.6.5 | MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| objc2-encode | 4.1.0 | MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| objc2-exception-helper | 0.1.1 | Zlib OR Apache-2.0 OR MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| objc2-foundation | 0.3.2 | MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| objc2-web-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | the crate's authors, https://github.com/madsmtm/objc2 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | the crate's authors, https://github.com/matklad/once_cell |
| option-ext | 0.2.0 | MPL-2.0 | the crate's authors, https://github.com/soc/option-ext.git |
| ort | 2.0.0-rc.10 | MIT OR Apache-2.0 | Copyright (c) 2023-2025 pyke.io; Copyright (c) 2020 Nicolas Bigaouette |
| ort-sys | 2.0.0-rc.10 | MIT OR Apache-2.0 | Copyright (c) 2023-2025 pyke.io; Copyright (c) 2020 Nicolas Bigaouette |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | Copyright (c) 2016 The Rust Project Developers |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | Copyright (c) 2016 The Rust Project Developers |
| pem-rfc7468 | 1.0.0 | Apache-2.0 OR MIT | Copyright (c) 2021-2025 The RustCrypto Project Developers |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | Copyright (c) 2013-2025 The rust-url developers |
| phf | 0.13.1 | MIT | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi; COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER |
| phf_codegen | 0.13.1 | MIT | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi; COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER |
| phf_generator | 0.13.1 | MIT | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi; COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER |
| phf_macros | 0.13.1 | MIT | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi; COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER |
| phf_shared | 0.13.1 | MIT | Copyright (c) 2014-2022 Steven Fackler, Yuki Okushi; COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | the crate's authors, https://github.com/taiki-e/pin-project-lite |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | Copyright (c) 2014 Alex Crichton |
| plist | 1.8.0 | MIT | Copyright (c) 2015 Edward Barnard |
| png | 0.17.16 | MIT OR Apache-2.0 | Copyright (c) 2015 nwin |
| png | 0.18.1 | MIT OR Apache-2.0 | Copyright (c) 2015 nwin |
| pocket-resources | 0.3.2 | MIT | the crate's authors, https://github.com/tomaka/pocket-resources |
| potential_utf | 0.1.6 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 | Copyright 2023 Jacob Pratt et al.; Copyright (c) 2023 Jacob Pratt et al. |
| precomputed-hash | 0.1.1 | MIT | Copyright (c) 2017 Emilio Cobos Álvarez |
| primal-check | 0.3.4 | MIT OR Apache-2.0 | Copyright (c) 2014 Huon Wilson |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/proc-macro2 |
| quick-xml | 0.38.4 | MIT | Copyright (c) 2016 Johann Tuffe |
| quote | 1.0.47 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/quote |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | Copyright (c) 2019 Osspial; Copyright (c) 2020 Osspial |
| rawpointer | 0.2.1 | MIT/Apache-2.0 | Copyright (c) 2015 |
| realfft | 3.5.0 | MIT | the crate's authors, https://github.com/HEnquist/realfft |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/ref-cast |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/ref-cast |
| regex | 1.13.1 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | Copyright (c) 2014 The Rust Project Developers |
| ring | 0.17.14 | Apache-2.0 AND ISC | Copyright (c) 2009 The Go Authors. All rights reserved.; Copyright 2015 The Chromium Authors. All rights reserved.; Copyright 2015-2025 Brian Smith. |
| rtrb | 0.3.5 | MIT OR Apache-2.0 | the crate's authors, https://github.com/mgeier/rtrb |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | the crate's authors, https://github.com/rust-lang/rustc-hash |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | Copyright (c) 2016 The Rust Project Developers |
| rustfft | 6.4.1 | MIT OR Apache-2.0 | Copyright (c) 2015 The RustFFT Developers |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Copyrights in the `rustix` project are retained by their contributors. |
| rustls | 0.23.45 | Apache-2.0 OR ISC OR MIT | Copyright (c) 2016, Joseph Birr-Pixton <jpixton@gmail.com>; Copyright (c) 2016 Joseph Birr-Pixton <jpixton@gmail.com> |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | Copyright 2023 Dirkjan Ochtman; Copyright (c) 2023 Dirkjan Ochtman <dirkjan@ochtman.nl> |
| rustls-webpki | 0.103.15 | ISC | Copyright 2015 Brian Smith. |
| same-file | 1.0.6 | Unlicense/MIT | Copyright (c) 2017 Andrew Gallant |
| schemars | 0.8.22 | MIT | Copyright (c) 2019 Graham Esau |
| schemars | 0.9.0 | MIT | Copyright (c) 2019 Graham Esau |
| schemars | 1.2.3 | MIT | Copyright (c) 2019 Graham Esau |
| schemars_derive | 0.8.22 | MIT | Copyright (c) 2019 Graham Esau |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | Copyright (c) 2016-2019 Ulrik Sverdrup "bluss" and scopeguard developers |
| security-framework | 3.7.0 | MIT OR Apache-2.0 | Copyright (c) 2015 Steven Fackler; COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 | Copyright (c) 2015 Steven Fackler; COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER |
| selectors | 0.38.0 | MPL-2.0 | the crate's authors, https://github.com/servo/stylo |
| semver | 1.0.28 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/semver |
| serde | 1.0.229 | MIT OR Apache-2.0 | the crate's authors, https://github.com/serde-rs/serde |
| serde-untagged | 0.1.9 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/serde-untagged |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | the crate's authors, https://github.com/serde-rs/serde |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | the crate's authors, https://github.com/serde-rs/serde |
| serde_derive_internals | 0.29.1 | MIT OR Apache-2.0 | the crate's authors, https://github.com/serde-rs/serde |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | the crate's authors, https://github.com/serde-rs/json |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/serde-repr |
| serde_spanned | 1.1.2 | MIT OR Apache-2.0 | Copyright Individual contributors; Copyright (c) Individual contributors |
| serde_with | 3.17.0 | MIT OR Apache-2.0 | Copyright (c) 2015 |
| serde_with_macros | 3.17.0 | MIT OR Apache-2.0 | Copyright (c) 2015 |
| serialize-to-javascript | 0.1.2 | MIT OR Apache-2.0 | Copyright (c) 2021 Chip Reed |
| serialize-to-javascript-impl | 0.1.2 | MIT OR Apache-2.0 | Copyright (c) 2021 Chip Reed |
| servo_arc | 0.4.3 | MIT OR Apache-2.0 | the crate's authors, https://github.com/servo/stylo |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | Copyright (c) 2006-2009 Graydon Hoare; Copyright (c) 2009-2013 Mozilla Foundation; Copyright (c) 2016 Artyom Pavlov |
| shlex | 2.0.1 | MIT OR Apache-2.0 | Copyright 2015 Nicholas Allegra (comex).; Copyright (c) 2015 Nicholas Allegra (comex). |
| simd-adler32 | 0.3.10 | MIT | Copyright (c) [2021] [Marvin Countryman] |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | Copyright 2012-2016 The Rust Project Developers.; Copyright 2016-2026 Frank Denis. |
| smallvec | 1.16.3 | MIT OR Apache-2.0 | Copyright (c) 2018 The Servo Project Developers |
| smallvec | 2.0.0-alpha.10 | MIT OR Apache-2.0 | Copyright (c) 2018 The Servo Project Developers |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | Copyright (c) 2014 Alex Crichton |
| socks | 0.3.4 | MIT/Apache-2.0 | Copyright (c) 2015 The rust-socks Developers |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | Copyright (c) 2017 Robert Grosse |
| strength_reduce | 0.2.4 | MIT OR Apache-2.0 | Copyright (c) 2015 The RustFFT Developers |
| string_cache | 0.9.0 | MIT OR Apache-2.0 | Copyright (c) 2012-2013 Mozilla Foundation |
| string_cache_codegen | 0.6.1 | MIT OR Apache-2.0 | Copyright (c) 2012-2013 Mozilla Foundation |
| strsim | 0.11.1 | MIT | Copyright (c) 2015 Danny Guo; Copyright (c) 2016 Titus Wormer <tituswormer@gmail.com>; Copyright (c) 2018 Akash Kurdekar |
| subtle | 2.6.1 | BSD-3-Clause | Copyright (c) 2016-2017 Isis Agora Lovecruft, Henry de Valence. All rights reserved.; Copyright (c) 2016-2024 Isis Agora Lovecruft. All rights reserved. |
| swift-rs | 1.0.9 | MIT OR Apache-2.0 | Copyright 2023 The swift-rs developers; Copyright (c) 2023 The swift-rs Developers |
| syn | 2.0.119 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/syn |
| syn | 3.0.9 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/syn |
| synstructure | 0.14.0 | MIT | Copyright 2016 Nika Layzell |
| tao | 0.37.1 | Apache-2.0 | the crate's authors, https://github.com/tauri-apps/tao |
| tar | 0.4.46 | MIT OR Apache-2.0 | Copyright (c) The tar-rs Project Contributors |
| tauri | 2.12.3 | Apache-2.0 OR MIT | Copyright (c) 2017 - Present Tauri Apps Contributors |
| tauri-build | 2.7.2 | Apache-2.0 OR MIT | Copyright (c) 2017 - Present Tauri Apps Contributors |
| tauri-codegen | 2.7.2 | Apache-2.0 OR MIT | Copyright (c) 2017 - Present Tauri Apps Contributors |
| tauri-macros | 2.7.2 | Apache-2.0 OR MIT | Copyright (c) 2017 - Present Tauri Apps Contributors |
| tauri-runtime | 2.12.1 | Apache-2.0 OR MIT | Copyright (c) 2017 - Present Tauri Apps Contributors |
| tauri-runtime-wry | 2.12.1 | Apache-2.0 OR MIT | Copyright (c) 2017 - Present Tauri Apps Contributors |
| tauri-utils | 2.10.1 | Apache-2.0 OR MIT | Copyright (c) 2017 - Present Tauri Apps Contributors |
| tauri-winres | 0.3.6 | MIT | Copyright (c) 2023 - Present Tauri Apps Contributors; Copyright (c) 2016 Max Resch |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | Copyright (c) 2015 Steven Allen |
| tendril | 0.5.1 | MIT OR Apache-2.0 | Copyright (c) 2015 Keegan McAllister |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/thiserror |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/thiserror |
| time | 0.3.41 | MIT OR Apache-2.0 | Copyright (c) Jacob Pratt et al. |
| time-core | 0.1.4 | MIT OR Apache-2.0 | Copyright (c) Jacob Pratt et al. |
| time-macros | 0.2.22 | MIT OR Apache-2.0 | Copyright (c) Jacob Pratt et al. |
| tinystr | 0.8.4 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| tokio | 1.53.2 | MIT | Copyright (c) Tokio Contributors |
| toml | 1.1.8+spec-1.1.0 | MIT OR Apache-2.0 | Copyright Individual contributors; Copyright (c) Individual contributors |
| toml_datetime | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | Copyright Individual contributors; Copyright (c) Individual contributors |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 | Copyright Individual contributors; Copyright (c) Individual contributors |
| toml_writer | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | Copyright Individual contributors; Copyright (c) Individual contributors |
| tracing | 0.1.44 | MIT | Copyright (c) 2019 Tokio Contributors |
| tracing-core | 0.1.36 | MIT | Copyright (c) 2019 Tokio Contributors |
| transpose | 0.2.3 | MIT OR Apache-2.0 | Copyright 2022 The transpose developers; Copyright (c) 2022 The transpose Developers |
| tray-icon | 0.25.1 | MIT OR Apache-2.0 | Copyright (c) 2022-2022 Tauri Programme within The Commons Conservancy |
| typeid | 1.0.3 | MIT OR Apache-2.0 | the crate's authors, https://github.com/dtolnay/typeid |
| typenum | 1.20.1 | MIT OR Apache-2.0 | Copyright 2014 Paho Lurie-Gregg; Copyright (c) 2014 Paho Lurie-Gregg |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 1991-2023 Unicode, Inc. |
| untrusted | 0.9.0 | ISC | the crate's authors, https://github.com/briansmith/untrusted |
| ureq | 2.12.1 | MIT OR Apache-2.0 | Copyright (c) 2019 Martin Algesten |
| ureq | 3.4.2 | MIT OR Apache-2.0 | Copyright (c) 2019 Martin Algesten |
| ureq-proto | 0.6.4 | MIT OR Apache-2.0 | Copyright 2022 Martin Algesten |
| url | 2.5.8 | MIT OR Apache-2.0 | Copyright (c) 2013-2025 The rust-url developers |
| urlencoding | 2.1.3 | MIT | © 2016 Bertram Truong; © 2021 Kornel Lesiński |
| urlpattern | 0.6.0 | MIT | Copyright (c) 2021 the Deno authors |
| utf8-zero | 0.8.1 | MIT OR Apache-2.0 | the crate's authors, https://github.com/algesten/utf8-zero |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | Copyright Mozilla Foundation |
| uuid | 1.28.0 | Apache-2.0 OR MIT | Copyright (c) 2014 The Rust Project Developers; Copyright (c) 2018 Ashley Mannix, Christopher Armstrong, Dylan DPC, Hunar Roop Kahlon |
| version_check | 0.9.5 | MIT/Apache-2.0 | Copyright (c) 2017-2018 Sergio Benitez; COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER |
| walkdir | 2.5.0 | Unlicense/MIT | Copyright (c) 2015 Andrew Gallant |
| web-time | 1.1.0 | MIT OR Apache-2.0 | Copyright 2023 dAxpeDDa; Copyright (c) 2023 dAxpeDDa |
| web_atoms | 0.2.6 | MIT OR Apache-2.0 | Copyright (c) 2014 The html5ever Project Developers |
| webpki-root-certs | 1.0.9 | CDLA-Permissive-2.0 | the crate's authors, https://github.com/rustls/webpki-roots |
| webpki-roots | 0.26.11 | CDLA-Permissive-2.0 | the crate's authors, https://github.com/rustls/webpki-roots |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | the crate's authors, https://github.com/rustls/webpki-roots |
| window-vibrancy | 0.8.1 | Apache-2.0 OR MIT | Copyright (c) 2020-2022 Tauri Programme within The Commons Conservancy |
| winnow | 1.0.4 | MIT | the crate's authors, https://github.com/winnow-rs/winnow |
| writeable | 0.6.4 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| wry | 0.57.0 | Apache-2.0 OR MIT | Copyright (c) 2020-2023 Ngo Iok Ui & Tauri Programme within The Commons Conservancy |
| xattr | 1.6.1 | MIT OR Apache-2.0 | Copyright (c) 2015 Steven Allen |
| yoke | 0.8.3 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| yoke-derive | 0.8.4 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| zerofrom | 0.1.8 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | Copyright (c) 2018-2026 The RustCrypto Project Developers |
| zerotrie | 0.2.5 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| zerovec | 0.11.8 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| zerovec-derive | 0.11.6 | Unicode-3.0 | COPYRIGHT AND PERMISSION NOTICE; Copyright © 2020-2024 Unicode, Inc. |
| zlib-rs | 0.6.8 | Zlib | (C) 2024 Trifecta Tech Foundation |
| zmij | 1.0.23 | MIT | the crate's authors, https://github.com/dtolnay/zmij |

## Appendix: licence texts

### MIT License

Used by: The models above, ONNX Runtime, most npm packages, and every crate listed as MIT or offering MIT as a choice. Each copyright holder is named above.

```
MIT License

Copyright (c) <year> <copyright holders>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### BSD 3-Clause License

Used by: alloc-no-stdlib, alloc-stdlib, brotli (with MIT), subtle, and others listed as BSD-3-Clause.

```
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

1. Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright
notice, this list of conditions and the following disclaimer in the
documentation and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
contributors may be used to endorse or promote products derived from
this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED
TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE. 
```

### ISC License

Used by: untrusted, rustls-webpki, ring (with Apache-2.0).

```
Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHORS DISCLAIM ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

### zlib License

Used by: foldhash, zlib-rs.

```
This software is provided 'as-is', without any express or implied warranty. In
no event will the authors be held liable for any damages arising from the use of
this software.

Permission is granted to anyone to use this software for any purpose, including
commercial applications, and to alter it and redistribute it freely, subject to
the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim
    that you wrote the original software. If you use this software in a product,
    an acknowledgment in the product documentation would be appreciated but is
    not required.

2. Altered source versions must be plainly marked as such, and must not be
    misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

### Unicode License v3

Used by: ICU4X crates (icu_*, litemap, tinystr, writeable, yoke, zerofrom, zerotrie, zerovec, potential_utf) and unicode-ident.

```
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 2020-2024 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.

SPDX-License-Identifier: Unicode-3.0

—

Portions of ICU4X may have been adapted from ICU4C and/or ICU4J.
ICU 1.8.1 to ICU 57.1 © 1995-2016 International Business Machines Corporation and others.
```

### Community Data License Agreement - Permissive 2.0

Used by: webpki-roots and webpki-root-certs (Mozilla's root certificate data).

```
# Community Data License Agreement - Permissive - Version 2.0

This is the Community Data License Agreement - Permissive, Version
2.0 (the "agreement"). Data Provider(s) and Data Recipient(s) agree
as follows:

## 1. Provision of the Data

1.1. A Data Recipient may use, modify, and share the Data made
available by Data Provider(s) under this agreement if that Data
Recipient follows the terms of this agreement.

1.2. This agreement does not impose any restriction on a Data
Recipient's use, modification, or sharing of any portions of the
Data that are in the public domain or that may be used, modified,
or shared under any other legal exception or limitation.

## 2. Conditions for Sharing Data

2.1. A Data Recipient may share Data, with or without modifications, so
long as the Data Recipient makes available the text of this agreement
with the shared Data.

## 3. No Restrictions on Results

3.1. This agreement does not impose any restriction or obligations
with respect to the use, modification, or sharing of Results.

## 4. No Warranty; Limitation of Liability

4.1. All Data Recipients receive the Data subject to the following
terms:

THE DATA IS PROVIDED ON AN "AS IS" BASIS, WITHOUT REPRESENTATIONS,
WARRANTIES OR CONDITIONS OF ANY KIND, EITHER EXPRESS OR IMPLIED
INCLUDING, WITHOUT LIMITATION, ANY WARRANTIES OR CONDITIONS OF TITLE,
NON-INFRINGEMENT, MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.

NO DATA PROVIDER SHALL HAVE ANY LIABILITY FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING
WITHOUT LIMITATION LOST PROFITS), HOWEVER CAUSED AND ON ANY THEORY OF
LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE DATA OR RESULTS,
EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

## 5. Definitions

5.1. "Data" means the material received by a Data Recipient under
this agreement.

5.2. "Data Provider" means any person who is the source of Data
provided under this agreement and in reliance on a Data Recipient's
agreement to its terms.

5.3. "Data Recipient" means any person who receives Data directly
or indirectly from a Data Provider and agrees to the terms of this
agreement.

5.4. "Results" means any outcome obtained by computational analysis
of Data, including for example machine learning models and models'
insights.
```

### Apache License 2.0

Used by: tao, ring, dpi, @swc/helpers, and any crate listed as Apache-2.0 only.

```
                              Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS
```

### Mozilla Public License 2.0

Used by: cssparser, cssparser-macros, selectors, dtoa-short, option-ext. Used unmodified; their source is available from crates.io and the repositories listed above.

```
Mozilla Public License Version 2.0
==================================

1. Definitions
--------------

1.1. "Contributor"
    means each individual or legal entity that creates, contributes to
    the creation of, or owns Covered Software.

1.2. "Contributor Version"
    means the combination of the Contributions of others (if any) used
    by a Contributor and that particular Contributor's Contribution.

1.3. "Contribution"
    means Covered Software of a particular Contributor.

1.4. "Covered Software"
    means Source Code Form to which the initial Contributor has attached
    the notice in Exhibit A, the Executable Form of such Source Code
    Form, and Modifications of such Source Code Form, in each case
    including portions thereof.

1.5. "Incompatible With Secondary Licenses"
    means

    (a) that the initial Contributor has attached the notice described
        in Exhibit B to the Covered Software; or

    (b) that the Covered Software was made available under the terms of
        version 1.1 or earlier of the License, but not also under the
        terms of a Secondary License.

1.6. "Executable Form"
    means any form of the work other than Source Code Form.

1.7. "Larger Work"
    means a work that combines Covered Software with other material, in 
    a separate file or files, that is not Covered Software.

1.8. "License"
    means this document.

1.9. "Licensable"
    means having the right to grant, to the maximum extent possible,
    whether at the time of the initial grant or subsequently, any and
    all of the rights conveyed by this License.

1.10. "Modifications"
    means any of the following:

    (a) any file in Source Code Form that results from an addition to,
        deletion from, or modification of the contents of Covered
        Software; or

    (b) any new file in Source Code Form that contains any Covered
        Software.

1.11. "Patent Claims" of a Contributor
    means any patent claim(s), including without limitation, method,
    process, and apparatus claims, in any patent Licensable by such
    Contributor that would be infringed, but for the grant of the
    License, by the making, using, selling, offering for sale, having
    made, import, or transfer of either its Contributions or its
    Contributor Version.

1.12. "Secondary License"
    means either the GNU General Public License, Version 2.0, the GNU
    Lesser General Public License, Version 2.1, the GNU Affero General
    Public License, Version 3.0, or any later versions of those
    licenses.

1.13. "Source Code Form"
    means the form of the work preferred for making modifications.

1.14. "You" (or "Your")
    means an individual or a legal entity exercising rights under this
    License. For legal entities, "You" includes any entity that
    controls, is controlled by, or is under common control with You. For
    purposes of this definition, "control" means (a) the power, direct
    or indirect, to cause the direction or management of such entity,
    whether by contract or otherwise, or (b) ownership of more than
    fifty percent (50%) of the outstanding shares or beneficial
    ownership of such entity.

2. License Grants and Conditions
--------------------------------

2.1. Grants

Each Contributor hereby grants You a world-wide, royalty-free,
non-exclusive license:

(a) under intellectual property rights (other than patent or trademark)
    Licensable by such Contributor to use, reproduce, make available,
    modify, display, perform, distribute, and otherwise exploit its
    Contributions, either on an unmodified basis, with Modifications, or
    as part of a Larger Work; and

(b) under Patent Claims of such Contributor to make, use, sell, offer
    for sale, have made, import, and otherwise transfer either its
    Contributions or its Contributor Version.

2.2. Effective Date

The licenses granted in Section 2.1 with respect to any Contribution
become effective for each Contribution on the date the Contributor first
distributes such Contribution.

2.3. Limitations on Grant Scope

The licenses granted in this Section 2 are the only rights granted under
this License. No additional rights or licenses will be implied from the
distribution or licensing of Covered Software under this License.
Notwithstanding Section 2.1(b) above, no patent license is granted by a
Contributor:

(a) for any code that a Contributor has removed from Covered Software;
    or

(b) for infringements caused by: (i) Your and any other third party's
    modifications of Covered Software, or (ii) the combination of its
    Contributions with other software (except as part of its Contributor
    Version); or

(c) under Patent Claims infringed by Covered Software in the absence of
    its Contributions.

This License does not grant any rights in the trademarks, service marks,
or logos of any Contributor (except as may be necessary to comply with
the notice requirements in Section 3.4).

2.4. Subsequent Licenses

No Contributor makes additional grants as a result of Your choice to
distribute the Covered Software under a subsequent version of this
License (see Section 10.2) or under the terms of a Secondary License (if
permitted under the terms of Section 3.3).

2.5. Representation

Each Contributor represents that the Contributor believes its
Contributions are its original creation(s) or it has sufficient rights
to grant the rights to its Contributions conveyed by this License.

2.6. Fair Use

This License is not intended to limit any rights You have under
applicable copyright doctrines of fair use, fair dealing, or other
equivalents.

2.7. Conditions

Sections 3.1, 3.2, 3.3, and 3.4 are conditions of the licenses granted
in Section 2.1.

3. Responsibilities
-------------------

3.1. Distribution of Source Form

All distribution of Covered Software in Source Code Form, including any
Modifications that You create or to which You contribute, must be under
the terms of this License. You must inform recipients that the Source
Code Form of the Covered Software is governed by the terms of this
License, and how they can obtain a copy of this License. You may not
attempt to alter or restrict the recipients' rights in the Source Code
Form.

3.2. Distribution of Executable Form

If You distribute Covered Software in Executable Form then:

(a) such Covered Software must also be made available in Source Code
    Form, as described in Section 3.1, and You must inform recipients of
    the Executable Form how they can obtain a copy of such Source Code
    Form by reasonable means in a timely manner, at a charge no more
    than the cost of distribution to the recipient; and

(b) You may distribute such Executable Form under the terms of this
    License, or sublicense it under different terms, provided that the
    license for the Executable Form does not attempt to limit or alter
    the recipients' rights in the Source Code Form under this License.

3.3. Distribution of a Larger Work

You may create and distribute a Larger Work under terms of Your choice,
provided that You also comply with the requirements of this License for
the Covered Software. If the Larger Work is a combination of Covered
Software with a work governed by one or more Secondary Licenses, and the
Covered Software is not Incompatible With Secondary Licenses, this
License permits You to additionally distribute such Covered Software
under the terms of such Secondary License(s), so that the recipient of
the Larger Work may, at their option, further distribute the Covered
Software under the terms of either this License or such Secondary
License(s).

3.4. Notices

You may not remove or alter the substance of any license notices
(including copyright notices, patent notices, disclaimers of warranty,
or limitations of liability) contained within the Source Code Form of
the Covered Software, except that You may alter any license notices to
the extent required to remedy known factual inaccuracies.

3.5. Application of Additional Terms

You may choose to offer, and to charge a fee for, warranty, support,
indemnity or liability obligations to one or more recipients of Covered
Software. However, You may do so only on Your own behalf, and not on
behalf of any Contributor. You must make it absolutely clear that any
such warranty, support, indemnity, or liability obligation is offered by
You alone, and You hereby agree to indemnify every Contributor for any
liability incurred by such Contributor as a result of warranty, support,
indemnity or liability terms You offer. You may include additional
disclaimers of warranty and limitations of liability specific to any
jurisdiction.

4. Inability to Comply Due to Statute or Regulation
---------------------------------------------------

If it is impossible for You to comply with any of the terms of this
License with respect to some or all of the Covered Software due to
statute, judicial order, or regulation then You must: (a) comply with
the terms of this License to the maximum extent possible; and (b)
describe the limitations and the code they affect. Such description must
be placed in a text file included with all distributions of the Covered
Software under this License. Except to the extent prohibited by statute
or regulation, such description must be sufficiently detailed for a
recipient of ordinary skill to be able to understand it.

5. Termination
--------------

5.1. The rights granted under this License will terminate automatically
if You fail to comply with any of its terms. However, if You become
compliant, then the rights granted under this License from a particular
Contributor are reinstated (a) provisionally, unless and until such
Contributor explicitly and finally terminates Your grants, and (b) on an
ongoing basis, if such Contributor fails to notify You of the
non-compliance by some reasonable means prior to 60 days after You have
come back into compliance. Moreover, Your grants from a particular
Contributor are reinstated on an ongoing basis if such Contributor
notifies You of the non-compliance by some reasonable means, this is the
first time You have received notice of non-compliance with this License
from such Contributor, and You become compliant prior to 30 days after
Your receipt of the notice.

5.2. If You initiate litigation against any entity by asserting a patent
infringement claim (excluding declaratory judgment actions,
counter-claims, and cross-claims) alleging that a Contributor Version
directly or indirectly infringes any patent, then the rights granted to
You by any and all Contributors for the Covered Software under Section
2.1 of this License shall terminate.

5.3. In the event of termination under Sections 5.1 or 5.2 above, all
end user license agreements (excluding distributors and resellers) which
have been validly granted by You or Your distributors under this License
prior to termination shall survive termination.

************************************************************************
*                                                                      *
*  6. Disclaimer of Warranty                                           *
*  -------------------------                                           *
*                                                                      *
*  Covered Software is provided under this License on an "as is"       *
*  basis, without warranty of any kind, either expressed, implied, or  *
*  statutory, including, without limitation, warranties that the       *
*  Covered Software is free of defects, merchantable, fit for a        *
*  particular purpose or non-infringing. The entire risk as to the     *
*  quality and performance of the Covered Software is with You.        *
*  Should any Covered Software prove defective in any respect, You     *
*  (not any Contributor) assume the cost of any necessary servicing,   *
*  repair, or correction. This disclaimer of warranty constitutes an   *
*  essential part of this License. No use of any Covered Software is   *
*  authorized under this License except under this disclaimer.         *
*                                                                      *
************************************************************************

************************************************************************
*                                                                      *
*  7. Limitation of Liability                                          *
*  --------------------------                                          *
*                                                                      *
*  Under no circumstances and under no legal theory, whether tort      *
*  (including negligence), contract, or otherwise, shall any           *
*  Contributor, or anyone who distributes Covered Software as          *
*  permitted above, be liable to You for any direct, indirect,         *
*  special, incidental, or consequential damages of any character      *
*  including, without limitation, damages for lost profits, loss of    *
*  goodwill, work stoppage, computer failure or malfunction, or any    *
*  and all other commercial damages or losses, even if such party      *
*  shall have been informed of the possibility of such damages. This   *
*  limitation of liability shall not apply to liability for death or   *
*  personal injury resulting from such party's negligence to the       *
*  extent applicable law prohibits such limitation. Some               *
*  jurisdictions do not allow the exclusion or limitation of           *
*  incidental or consequential damages, so this exclusion and          *
*  limitation may not apply to You.                                    *
*                                                                      *
************************************************************************

8. Litigation
-------------

Any litigation relating to this License may be brought only in the
courts of a jurisdiction where the defendant maintains its principal
place of business and such litigation shall be governed by laws of that
jurisdiction, without reference to its conflict-of-law provisions.
Nothing in this Section shall prevent a party's ability to bring
cross-claims or counter-claims.

9. Miscellaneous
----------------

This License represents the complete agreement concerning the subject
matter hereof. If any provision of this License is held to be
unenforceable, such provision shall be reformed only to the extent
necessary to make it enforceable. Any law or regulation which provides
that the language of a contract shall be construed against the drafter
shall not be used to construe this License against a Contributor.

10. Versions of the License
---------------------------

10.1. New Versions

Mozilla Foundation is the license steward. Except as provided in Section
10.3, no one other than the license steward has the right to modify or
publish new versions of this License. Each version will be given a
distinguishing version number.

10.2. Effect of New Versions

You may distribute the Covered Software under the terms of the version
of the License under which You originally received the Covered Software,
or under the terms of any subsequent version published by the license
steward.

10.3. Modified Versions

If you create software not governed by this License, and you want to
create a new license for such software, you may create and use a
modified version of this License if you rename the license and remove
any references to the name of the license steward (except to note that
such modified license differs from this License).

10.4. Distributing Source Code Form that is Incompatible With Secondary
Licenses

If You choose to distribute Source Code Form that is Incompatible With
Secondary Licenses under the terms of this version of the License, the
notice described in Exhibit B of this License must be attached.

Exhibit A - Source Code Form License Notice
-------------------------------------------

  This Source Code Form is subject to the terms of the Mozilla Public
  License, v. 2.0. If a copy of the MPL was not distributed with this
  file, You can obtain one at http://mozilla.org/MPL/2.0/.

If it is not possible or desirable to put the notice in a particular
file, then You may include the notice in a location (such as a LICENSE
file in a relevant directory) where a recipient would be likely to look
for such a notice.

You may add additional accurate notices of copyright ownership.

Exhibit B - "Incompatible With Secondary Licenses" Notice
---------------------------------------------------------

  This Source Code Form is "Incompatible With Secondary Licenses", as
  defined by the Mozilla Public License, v. 2.0.
```

### SIL Open Font License 1.1

Used by: Geist and Geist Mono.

```
Copyright (c) 2023 Vercel, in collaboration with basement.studio

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL

-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION AND CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```
