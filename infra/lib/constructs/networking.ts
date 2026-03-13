import { Construct } from 'constructs';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import { FckNatInstanceProvider } from 'cdk-fck-nat';

export class Networking extends Construct {
  public readonly vpc: ec2.Vpc;
  public readonly rdsSecurityGroup: ec2.SecurityGroup;
  public readonly vpcConnectorSecurityGroup: ec2.SecurityGroup;

  constructor(scope: Construct, id: string) {
    super(scope, id);

    // fck-nat: t4g.nano EC2 instance acting as NAT (~$3/mo vs $32/mo managed).
    // NAT is required because App Runner VPC egress routes ALL outbound traffic
    // through the VPC, including OAuth calls to Google/GitHub APIs.
    // Single instance in 1 AZ — swap to managed NAT Gateway when revenue justifies it.
    const natProvider = new FckNatInstanceProvider({
      instanceType: ec2.InstanceType.of(ec2.InstanceClass.T4G, ec2.InstanceSize.NANO),
    });

    this.vpc = new ec2.Vpc(this, 'Vpc', {
      maxAzs: 2,
      natGateways: 1,
      natGatewayProvider: natProvider,
      subnetConfiguration: [
        {
          name: 'Public',
          subnetType: ec2.SubnetType.PUBLIC,
          cidrMask: 24,
        },
        {
          name: 'Private',
          subnetType: ec2.SubnetType.PRIVATE_WITH_EGRESS,
          cidrMask: 24,
        },
      ],
    });

    // Allow traffic from private subnets to reach the fck-nat instance
    natProvider.securityGroup.addIngressRule(
      ec2.Peer.ipv4(this.vpc.vpcCidrBlock),
      ec2.Port.allTraffic(),
      'Allow private subnet traffic through NAT',
    );

    // Security group for VPC Connector (App Runner → RDS + internet)
    this.vpcConnectorSecurityGroup = new ec2.SecurityGroup(this, 'VpcConnectorSg', {
      vpc: this.vpc,
      description: 'App Runner VPC Connector',
      allowAllOutbound: false,
    });

    // Security group for RDS
    this.rdsSecurityGroup = new ec2.SecurityGroup(this, 'RdsSg', {
      vpc: this.vpc,
      description: 'RDS PostgreSQL - inbound from VPC Connector only',
      allowAllOutbound: false,
    });

    // Allow VPC Connector → RDS on port 5432
    this.rdsSecurityGroup.addIngressRule(
      this.vpcConnectorSecurityGroup,
      ec2.Port.tcp(5432),
      'Allow PostgreSQL from App Runner VPC Connector',
    );

    this.vpcConnectorSecurityGroup.addEgressRule(
      this.rdsSecurityGroup,
      ec2.Port.tcp(5432),
      'Allow outbound to RDS PostgreSQL',
    );

    // Allow VPC Connector → internet on HTTPS (for OAuth provider APIs)
    this.vpcConnectorSecurityGroup.addEgressRule(
      ec2.Peer.anyIpv4(),
      ec2.Port.tcp(443),
      'Allow outbound HTTPS for OAuth provider APIs',
    );

    // Allow DNS lookups for outbound HTTPS destinations (OAuth providers, AWS endpoints)
    this.vpcConnectorSecurityGroup.addEgressRule(
      ec2.Peer.anyIpv4(),
      ec2.Port.udp(53),
      'Allow outbound DNS (UDP)',
    );
    this.vpcConnectorSecurityGroup.addEgressRule(
      ec2.Peer.anyIpv4(),
      ec2.Port.tcp(53),
      'Allow outbound DNS (TCP fallback)',
    );
  }
}
