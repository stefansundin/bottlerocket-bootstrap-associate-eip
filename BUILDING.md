To build a multi-arch docker image you can run:

```shell
# Simple build for one architecture:
docker build --pull --progress plain -t bottlerocket-bootstrap-associate-eip .

# Use buildx to build multi-arch images:
docker buildx create --use --name multiarch --node multiarch0

# Beta release (may be limited to one architecture):
docker buildx build --pull --push --progress plain --platform linux/arm64 -t public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:beta -f Dockerfile.debug .

# Build the debug image:
docker buildx build --pull --push --progress plain --platform linux/arm64,linux/amd64 -t public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:debug -f Dockerfile.debug .

# You probably want to change the tag name if you are not me:
docker buildx build --push --progress plain --platform linux/arm64,linux/amd64 -t public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:v0.2.1 .

# If the new version is stable then update latest:
docker buildx build --push --progress plain --platform linux/arm64,linux/amd64 -t public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:latest .
```

If the build crashes then it is most likely because Docker ran out of memory. Increase the amount of RAM allocated to Docker and quit other programs during the build.
