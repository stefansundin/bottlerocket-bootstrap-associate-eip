To build a multi-arch docker image you can run:

```shell
docker buildx create --use --name multiarch --node multiarch0

# You probably want to change the tag name if you are not me:
docker buildx build --pull --push --progress plain --platform linux/arm64,linux/amd64 -t public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:v0.1.0 .

# If the new version is stable then update latest:
docker buildx build --pull --push --progress plain --platform linux/arm64,linux/amd64 -t public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:latest .
```

If the build crashes then it is most likely because Docker ran out of memory. Increase the amount of RAM allocated to Docker and quit other programs during the build.
