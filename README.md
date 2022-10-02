This is a bootstrap container for Bottlerocket that associates an elastic IP address to the instance when it starts up the first time. This is useful if you want to run a single EC2 instance that maintains the same IP address even if the instance is replaced.

Because Bottlerocket doesn't allow for traditional startup scripts in the user data, you can't just run aws-cli commands like you may be used to. Bottlerocket provides a way to run bootstrap containers instead, which you can use to configure the system when it starts up.

A Rust program compiled using musl was picked to minimize the size of the docker image (the docker image is about 3 MB compressed). It is published on Amazon Public ECR: https://gallery.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip

Here's how to configure it in your Bottlerocket user data:

```toml
[settings.bootstrap-containers.associate-eip]
source = "public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:latest"
mode = "once"
essential = false
user-data = "ZWlwYWxsb2MtMDEyMzQ1Njc4OTBhYmNkZWYK"
```

The `user-data` contains the Elastic IP Allocation ID that you want to associate with the instance, encoded using base64. You can generate it like this:

```shell
echo eipalloc-01234567890abcdef | base64
```

### Additional options

There are additional features besides the simple use-case demonstrated above. To use the additional options you need to pass in a JSON string in the `user-data` instead of just the `eipalloc` identifier.

```shell
echo '{"AllocationId":"eipalloc-01234567890abcdef","AllowReassociation":true}' | base64
```

- `AllocationId` is required.
- `AllowReassociation` is `true` if omitted.


## Feedback

This is one of my first Rust programs. If you have ideas for improvements, please open an issue or a pull request.

If you have questions then please open an issue or a discussion.


## Supported Bottlerocket versions

Tested on Bottlerocket v1.9.2.


## IAM Permissions

Make sure your instance has permissions to associate elastic IP addresses.

```json
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "ec2:AssociateAddress"
            ],
            "Resource": "*"
        }
    ]
}
```


## Troubleshooting

You can get the output from the docker container by running:

```shell
enter-admin-container
sheltie
journalctl -u bootstrap-containers@associate-eip.service
```

If you are unable to connect to the instance then wait 10 minutes and then check the EC2 instance system log.

If you want more logging enabled then use the `debug` image:

```toml
source = "public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:debug"
```


## Developing

There is an integration test that simulates the required environment.

```
export RUST_LOG=aws
cargo test -- --nocapture
```
