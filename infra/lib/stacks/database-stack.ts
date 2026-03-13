import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as rds from 'aws-cdk-lib/aws-rds';
import { Construct } from 'constructs';
import { Database } from '../constructs/database';

export interface DatabaseStackProps extends cdk.StackProps {
  vpc: ec2.Vpc;
  rdsSecurityGroup: ec2.SecurityGroup;
  databaseName: string;
  credentialsSecretName: string;
  applyLegacySecretLogicalIdOverride?: boolean;
}

export class DatabaseStack extends cdk.Stack {
  public readonly instance: rds.DatabaseInstance;

  constructor(scope: Construct, id: string, props: DatabaseStackProps) {
    super(scope, id, props);

    const database = new Database(this, 'Database', {
      vpc: props.vpc,
      securityGroup: props.rdsSecurityGroup,
      databaseName: props.databaseName,
      credentialsSecretName: props.credentialsSecretName,
      applyLegacySecretLogicalIdOverride: props.applyLegacySecretLogicalIdOverride,
    });

    this.instance = database.instance;
  }
}
