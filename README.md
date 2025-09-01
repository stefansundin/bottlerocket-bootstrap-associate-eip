This is a bootstrap container for Bottlerocket that associates an elastic IP address to the EC2 instance when it starts up. This is useful if you want to run an EC2 instance that maintains the same IP address even if it is replaced. You can also associate an address from a pool of elastic IP addresses (see below for additional options).

Because Bottlerocket doesn't allow for traditional startup scripts in the user data, you can't just run aws-cli commands like you may be used to. Bottlerocket provides a way to run bootstrap containers instead, which you can use to configure the system when it starts up.

A Rust program compiled using musl was picked to minimize the size of the docker image (the docker image is about 4 MB compressed). It is published on Amazon Public ECR: https://gallery.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip

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

### Private IPv4 address

You can also automatically assign a private IPv4 address, just specify the desired IPv4 address in the user data:

```shell
echo 10.3.0.10 | base64
```

### IPv6 address

You can also automatically assign an IPv6 address, just specify the desired IPv6 address in the user data:

```shell
echo 2600:1f14:a11b:f301::a | base64
```

If you use this then you need to ensure that the EC2 instance is launched in the correct subnet.

### Additional options

There are additional options available when associating an EIP, besides the simple use-case demonstrated above. To use the additional options you need to pass in a JSON string in the `user-data` instead of just the `eipalloc` identifier.

```shell
echo '{"AllocationId":"eipalloc-01234567890abcdef","AllowReassociation":true}' | base64
```

If you want to dynamically find an EIP to use, e.g. based on tags, then you can use the `Filters` option (equivalent to [`--filters`](https://awscli.amazonaws.com/v2/documentation/api/latest/reference/ec2/describe-addresses.html#options) for `aws ec2 describe-addresses`):

```shell
echo '{"Filters":[{"Name":"tag:Pool","Values":["ecs"]}]}' | base64
```

> [!WARNING]
> When `Filters` is used, the program will try to pick an unallocated EIP at random. If all the EIPs are in use then one will be chosen at random anyway. Set `AllowReassociation` to `false` to exit with an error instead.

You can specify an empty array to have the program pick any EIP in the account:

```shell
echo '{"Filters":[]}' | base64
```

Reference:

- Either `AllocationId` or `Filters` is required.
- `AllowReassociation` is `true` if omitted.


## Supported Bottlerocket versions

Tested on Bottlerocket v1.9.2, v1.10.0, v1.26.2, v1.34.0, and v1.44.0.


## IAM Permissions

Make sure your instance has permissions to associate elastic IP addresses.

```json
{
    "Version": "2012-10-17",
    "Statement": [
        {
            "Effect": "Allow",
            "Action": [
                "ec2:AssociateAddress",
                "ec2:AssignPrivateIpAddresses",
                "ec2:AssignIpv6Addresses",
                "ec2:DescribeAddresses"
            ],
            "Resource": "*"
        }
    ]
}
```

You only need `ec2:DescribeAddresses` if you want to use the `Filters` option.


## Troubleshooting

You can get the output from the docker container by running:

```shell
enter-admin-container
sudo sheltie
journalctl -u bootstrap-containers@associate-eip.service
```

If you are unable to connect to the instance then wait 10 minutes and then check the EC2 instance system log.

If you want more logging enabled then use the `debug` image:

```toml
source = "public.ecr.aws/stefansundin/bottlerocket-bootstrap-associate-eip:debug"
```


## Developing

There is an integration test that simulates the required environment.

```shell
export RUST_LOG=aws
cargo test -- --nocapture --test-threads=1
```


## Feedback

If you have ideas for improvements, please open an issue or a pull request.

If you have questions then please open an issue or a discussion.
