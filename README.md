# rust-earthly-template

A very rough attempt to make the checks done in [jonhoo/rust-ci-conf](https://github.com/jonhoo/rust-ci-conf)
work in Earthly so that it's easier to run all your exact Rust CI checks locally.

Of course, you could just use a Justfile or something like that, but I'm
partial to [Earthly](https://earthly.dev/) :).

#### Prerequisites

1. Install earthly: https://earthly.dev/get-earthly
2. Fork/create template
3. Set `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` Github environment secrets

#### Run Locally

Run these tests locally with:

```shell
earthly --ci +ci
```

Run scheduled jobs with:

```shell
earthly --ci +scheduled-ci
```

#### TODO

- [ ] Add more robust tests such as in https://github.com/jonhoo/rust-ci-conf/blob/main/.github/workflows/test.yml
- [ ] Add safety checks like https://github.com/jonhoo/rust-ci-conf/blob/main/.github/workflows/safety.yml
- [ ] Add nostd checks like in https://github.com/jonhoo/rust-ci-conf/blob/main/.github/workflows/nostd.yml
- [ ] Consider removing the boilerplate hello world Cargo.{toml,lock} and src/
