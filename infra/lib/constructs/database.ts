import { Construct } from 'constructs';
import * as cdk from 'aws-cdk-lib';
import * as rds from 'aws-cdk-lib/aws-rds';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import { Duration, RemovalPolicy } from 'aws-cdk-lib';

export interface DatabaseProps {
  vpc: ec2.Vpc;
  securityGroup: ec2.SecurityGroup;
  databaseName: string;
  credentialsSecretName: string;
  applyLegacySecretLogicalIdOverride?: boolean;
}

export class Database extends Construct {
  public readonly instance: rds.DatabaseInstance;

  constructor(scope: Construct, id: string, props: DatabaseProps) {
    super(scope, id);

    // Parameter group for connection limits
    const parameterGroup = new rds.ParameterGroup(this, 'Params', {
      engine: rds.DatabaseInstanceEngine.postgres({ version: rds.PostgresEngineVersion.VER_16 }),
      parameters: {
        'max_connections': '50',
      },
    });

    this.instance = new rds.DatabaseInstance(this, 'Instance', {
      engine: rds.DatabaseInstanceEngine.postgres({
        version: rds.PostgresEngineVersion.VER_16,
      }),
      instanceType: ec2.InstanceType.of(ec2.InstanceClass.T4G, ec2.InstanceSize.MICRO),
      vpc: props.vpc,
      vpcSubnets: { subnetType: ec2.SubnetType.PRIVATE_WITH_EGRESS },
      securityGroups: [props.securityGroup],

      // Credentials auto-stored in Secrets Manager
      credentials: rds.Credentials.fromGeneratedSecret('gatewasm', {
        secretName: props.credentialsSecretName,
      }),
      databaseName: props.databaseName,

      // Storage
      allocatedStorage: 20,
      maxAllocatedStorage: 50,
      storageType: rds.StorageType.GP3,
      storageEncrypted: true,

      // Availability
      multiAz: false,

      // Backups
      backupRetention: Duration.days(7),

      // Monitoring
      enablePerformanceInsights: true,
      performanceInsightRetention: rds.PerformanceInsightRetention.DEFAULT, // 7 days free

      deletionProtection: true,
      removalPolicy: RemovalPolicy.RETAIN,

      parameterGroup,
    });

    // MIGRATION: The RDS auto-generated secret's logical ID includes the stack
    // name in its hash. Override both the CfnSecret and CfnSecretTargetAttachment
    // to match the original GatewasmStack logical IDs so `cdk import` can adopt them.
    if (props.applyLegacySecretLogicalIdOverride === true) {
      const allChildren = this.instance.node.findAll();
      for (const child of allChildren) {
        if (child instanceof cdk.CfnResource) {
          if (child.cfnResourceType === 'AWS::SecretsManager::Secret') {
            child.overrideLogicalId(
              'GatewasmStackDatabaseInstanceSecretBC64EE913fdaad7efa858a3daf9490cf0a702aeb',
            );
          }
        }
      }
    }
  }
}
