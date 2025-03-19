You can build the docker image by running:

```shell
# Simple build for your current architecture:
docker build --pull --progress plain -t bottlerocket-bootstrap-associate-eip .
```

To build a multi-arch docker image you can run:

```shell
# Use buildx to build multi-arch images:
docker buildx create --use --name multiarch --node multiarch0

# Beta release (may be limited to one architecture):
docker buildx build --pull --push --progress plain --platform linux/arm64 -t public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:beta -f Dockerfile.debug .

# Build the debug image:
docker buildx build --pull --push --progress plain --platform linux/arm64,linux/amd64 -t public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:debug -f Dockerfile.debug .

# You probably want to change the tag name if you are not me:
docker buildx build --push --progress plain --platform linux/arm64,linux/amd64 -t public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:v0.2.3 .

# If the new version is stable then update latest:
docker buildx imagetools create -t public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:latest public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:v0.2.3
```

If the build crashes then it is most likely because Docker ran out of memory. Increase the amount of RAM allocated to Docker and quit other programs during the build. You can also try passing `--build-arg CARGO_BUILD_JOBS=1` to docker. To build one architecture at a time, [use `--config` when creating the builder instance](https://gist.github.com/stefansundin/fa1c1dd7a60ebe2f8a2aa6d32631b119).
