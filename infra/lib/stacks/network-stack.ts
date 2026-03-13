import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import { Construct } from 'constructs';
import { Networking } from '../constructs/networking';

export class NetworkStack extends cdk.Stack {
  public readonly vpc: ec2.Vpc;
  public readonly rdsSecurityGroup: ec2.SecurityGroup;
  public readonly vpcConnectorSecurityGroup: ec2.SecurityGroup;

  constructor(scope: Construct, id: string, props?: cdk.StackProps) {
    super(scope, id, props);

    const networking = new Networking(this, 'Networking');

    this.vpc = networking.vpc;
    this.rdsSecurityGroup = networking.rdsSecurityGroup;
    this.vpcConnectorSecurityGroup = networking.vpcConnectorSecurityGroup;
  }
}
