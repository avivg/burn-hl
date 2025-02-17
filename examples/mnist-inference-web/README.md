# MNIST Inference on Web

[![Demo up](https://img.shields.io/badge/demo-up-brightgreen)](https://burn-rs.github.io/demo)

This crate demonstrates how to run an MNIST-trained model in the browser for inference.

## Running

1. Build

   ```shell
   ./build-for-web.sh
   ```

2. Run the server

   ```shell
   ./run-server.sh
   ```

3. Open the [`http://localhost:8000/`](http://localhost:8000/) in the browser.

## Design

The inference components of `burn` with the `ndarray` backend can be built with `#![no_std]`. This
makes it possible to build and run the model with the `wasm32-unknown-unknown` target without a
special system library, such as [WASI](https://wasi.dev/). (See [Cargo.toml](./Cargo.toml) on how to
include burn dependencies without `std`).

For this demo, we use trained parameters (`model-4.json.gz`) and model (`model.rs`) from the
[`burn` MNIST example](https://github.com/burn-rs/burn/tree/main/examples/mnist).

During the build time `model-4.json.gz` is converted to
[`bincode`](https://github.com/bincode-org/bincode) (for compactness) and included as part of the
final wasm output. The MNIST model is initialized with trained weights from memory during the
runtime.

The inference API for JavaScript is exposed with the help of
[`wasm-bindgen`](https://github.com/rustwasm/wasm-bindgen)'s library and tools.

JavaScript (`index.js`) is used to transform hand-drawn digits to a format that the inference API
accepts. The transformation includes image cropping, scaling down, and converting it to grayscale
values.

## Model

Layers:

1. Input Image (28,28, 1ch)
2. `Conv2d`(3x3, 8ch), `BatchNorm2d`, `GELU`
3. `Conv2d`(3x3, 16ch), `BatchNorm2d`, `GELU`
4. `Conv2d`(3x3, 24ch), `BatchNorm2d`, `GELU`
5. `Linear`(11616, 32), `GELU`
6. `Linear`(32, 10)
7. Softmax Output

The total number of parameters is 376,952.

The model is trained with 4 epochs and the final test accuracy is 98.67%.

The training and hyper parameter information in can be found in
[`burn` MNIST example](https://github.com/burn-rs/burn/tree/main/examples/mnist).

## Comparison

The main differentiating factor of this example's approach (compiling rust model into wasm) and
other popular tools, such as [TensorFlow.js](https://www.tensorflow.org/js),
[ONNX Runtime JS](https://onnxruntime.ai/docs/tutorials/web/) and
[TVM Web](https://github.com/apache/tvm/tree/main/web) is the absence of runtime code. The rust
compiler optimizes and includes only used `burn` routines. 1,509,747 bytes out of Wasm's 1,866,491
byte file is the model's parameters. The rest of 356,744 bytes contain all the code (including
`burn`'s `nn` components, the data deserialization library, and math operations).

## Future Improvements

There are several planned enhancements in place:

- [#201](https://github.com/burn-rs/burn/issues/201) - Saving model's params in binary format. This
  will simplify the inference code.
- [#202](https://github.com/burn-rs/burn/issues/202) - Saving model's params in half-precision and
  loading back in full. This can be half the size of the wasm file.
- [#243](https://github.com/burn-rs/burn/issues/243) - New WebGPU backend would allow computation
  using GPU in the browser.
- [#1271](https://github.com/rust-ndarray/ndarray/issues/1271) -
  [WASM SIMD](https://github.com/WebAssembly/simd/blob/master/proposals/simd/SIMD.md) support in
  NDArray that can speed up computation on CPU.

## Acknowledgements

Two online MNIST demos inspired and helped build this demo:
[MNIST Draw](https://mco-mnist-draw-rwpxka3zaa-ue.a.run.app/) by Marc (@mco-gh) and
[MNIST Web Demo](https://ufal.mff.cuni.cz/~straka/courses/npfl129/2223/demos/mnist_web.html) (no
code was copied but helped tremendously with an implementation approach).

## Resources

1. [Rust 🦀 and WebAssembly](https://rustwasm.github.io/docs/book/)
2. [wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/)
