# Simtricks

Simtricks is a tool to run [Matricks](https://github.com/wymcg/matricks) plugins without a Raspberry Pi.

![simtricks](https://github.com/wymcg/simtricks/assets/3410869/6f2837c4-0783-4b46-b985-48c9c2aef1b1)


## Installation
- Install Rust and Cargo from the [Rust website](https://rustup.rs/)
- Run `cargo install simtricks`

## Usage
Simtricks is run from the command line. At a minimum, you must provide a plugin and the dimensions of the matrix:
```
simtricks --path <PATH_TO_PLUGIN> --width <WIDTH> --height <HEIGHT>
```

For a list of examples to try, check out the Matricks [example plugin](https://github.com/wymcg/matricks/tree/main/examples) page.
