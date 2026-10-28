import { Construct } from 'constructs';
import * as ecr from 'aws-cdk-lib/aws-ecr';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as apprunner from 'aws-cdk-lib/aws-apprunner';
import * as secretsmanager from 'aws-cdk-lib/aws-secretsmanager';
import * as rds from 'aws-cdk-lib/aws-rds';
import { CfnOutput } from 'aws-cdk-lib';

export interface KmsKeyring {
  version: number;
  max_access_lifetime: number;
  publication: { next_key_id: string; all_replicas_serving_since: number; evidence: string } | null;
  active: string;
  next: string;
  ssf: string;
  previous: Array<{ key_id: string; last_issued_at: number | null; retire_after: number | null; drain_receipt: string | null }>;
}

export interface ServiceProps {
  vpc: ec2.Vpc;
  vpcConnectorSecurityGroup: ec2.SecurityGroup;
  rdsInstance: rds.DatabaseInstance;
  imageTag: string;
  repositoryName: string;
  serviceName: string;
  ed25519Key?: secretsmanager.ISecret;
  kmsKeyring?: KmsKeyring;
  csrfSecret: secretsmanager.ISecret;
  platformAdminKey: secretsmanager.ISecret;
  oauthGoogle: secretsmanager.ISecret;
  oauthGithub: secretsmanager.ISecret;
  smtp?: secretsmanager.ISecret;
  mfaEncryptionKey?: secretsmanager.ISecret;
  ssfSigningKey?: secretsmanager.ISecret;
  serverUrl: string;
  platformDomain: string;
  platformOwnerEmails: string;
  allowedOrigins: string;
}

export class Service extends Construct {
  public readonly repository: ecr.IRepository;
  public readonly serviceUrl: string;
  public readonly serviceId: string;
  public readonly serviceName: string;

  constructor(scope: Construct, id: string, props: ServiceProps) {
    super(scope, id);

    // ECR Repository — created outside the stack to avoid chicken-and-egg:
    // App Runner needs an image in ECR to start, but the stack creates ECR.
    // Create repo first: aws ecr create-repository --repository-name gatewasm-server
    // Push image, then deploy this stack.
    this.repository = ecr.Repository.fromRepositoryName(this, 'Repo', props.repositoryName);

    // IAM role for App Runner to pull from ECR
    const accessRole = new iam.Role(this, 'AccessRole', {
      assumedBy: new iam.ServicePrincipal('build.apprunner.amazonaws.com'),
      description: 'App Runner ECR access role',
    });
    this.repository.grantPull(accessRole);

    // IAM role for the App Runner instance (reads secrets)
    const instanceRole = new iam.Role(this, 'InstanceRole', {
      assumedBy: new iam.ServicePrincipal('tasks.apprunner.amazonaws.com'),
      description: 'App Runner instance role',
    });

    // Grant read access to all secrets
    const secrets = [
      props.csrfSecret,
      props.platformAdminKey,
      props.oauthGoogle,
      props.oauthGithub,
    ];
    if (props.kmsKeyring && (props.ed25519Key || props.ssfSigningKey)) {
      throw new Error('KMS custody forbids process-readable signing secrets');
    }
    if (!props.kmsKeyring && !props.ed25519Key) throw new Error('Signing configuration is required');
    if (props.ed25519Key) secrets.push(props.ed25519Key);
    if (props.kmsKeyring) {
      const ring=props.kmsKeyring;
      const publicKeys=[ring.active,ring.next,ring.ssf,...ring.previous.map(key=>key.key_id)];
      if (new Set(publicKeys).size !== publicKeys.length || publicKeys.some(key=>!/^arn:aws:kms:[a-z0-9-]+:\d{12}:key\/[a-zA-Z0-9-]+$/.test(key))) throw new Error('KMS keyring requires distinct immutable key ARNs');
      instanceRole.addToPolicy(new iam.PolicyStatement({ actions:['kms:GetPublicKey','kms:DescribeKey'], resources:publicKeys }));
      instanceRole.addToPolicy(new iam.PolicyStatement({ actions:['kms:Sign'], resources:[ring.active,ring.ssf], conditions:{ StringEquals:{ 'kms:SigningAlgorithm':'ED25519_SHA_512', 'kms:MessageType':'RAW' } } }));
    }
    if (props.smtp) {
      secrets.push(props.smtp);
    }
    if (props.mfaEncryptionKey) {
      secrets.push(props.mfaEncryptionKey);
    }
    if (props.ssfSigningKey) {
      secrets.push(props.ssfSigningKey);
    }
    for (const secret of secrets) {
      secret.grantRead(instanceRole);
    }
    // Grant read access to RDS-generated credentials secret
    props.rdsInstance.secret!.grantRead(instanceRole);

    // VPC Connector — lets App Runner reach RDS in private subnets
    const vpcConnector = new apprunner.CfnVpcConnector(this, 'VpcConnector', {
      subnets: props.vpc.selectSubnets({
        subnetType: ec2.SubnetType.PRIVATE_WITH_EGRESS,
      }).subnetIds,
      securityGroups: [props.vpcConnectorSecurityGroup.securityGroupId],
      // Omit vpcConnectorName — let CloudFormation auto-generate to avoid
      // "already exists" conflicts during redeployment after failed rollbacks
    });

    // RDS secret ARN for field extraction
    const rdsSecretArn = props.rdsInstance.secret!.secretArn;
    const runtimeEnvironmentSecrets = [
      // RDS credentials (JSON field extraction)
      { name: 'DB_HOST', value: `${rdsSecretArn}:host::` },
      { name: 'DB_PORT', value: `${rdsSecretArn}:port::` },
      { name: 'DB_USERNAME', value: `${rdsSecretArn}:username::` },
      { name: 'DB_PASSWORD', value: `${rdsSecretArn}:password::` },
      { name: 'DB_NAME', value: `${rdsSecretArn}:dbname::` },
      // App secrets (plain string)
      { name: 'CSRF_SECRET', value: props.csrfSecret.secretArn },
      { name: 'PLATFORM_ADMIN_KEY', value: props.platformAdminKey.secretArn },
      // OAuth (JSON field extraction)
      { name: 'GOOGLE_CLIENT_ID', value: `${props.oauthGoogle.secretArn}:client_id::` },
      { name: 'GOOGLE_CLIENT_SECRET', value: `${props.oauthGoogle.secretArn}:client_secret::` },
      { name: 'GITHUB_CLIENT_ID', value: `${props.oauthGithub.secretArn}:client_id::` },
      { name: 'GITHUB_CLIENT_SECRET', value: `${props.oauthGithub.secretArn}:client_secret::` },
    ];
    if (props.ed25519Key) runtimeEnvironmentSecrets.push({ name:'ED25519_PRIVATE_KEY', value:props.ed25519Key.secretArn });
    if (props.smtp) {
      runtimeEnvironmentSecrets.push(
        { name: 'SMTP_HOST', value: `${props.smtp.secretArn}:host::` },
        { name: 'SMTP_USERNAME', value: `${props.smtp.secretArn}:username::` },
        { name: 'SMTP_PASSWORD', value: `${props.smtp.secretArn}:password::` },
        { name: 'SMTP_FROM_EMAIL', value: `${props.smtp.secretArn}:from_email::` },
      );
    }
    if (props.mfaEncryptionKey) {
      runtimeEnvironmentSecrets.push(
        { name: 'MFA_ENCRYPTION_KEY', value: props.mfaEncryptionKey.secretArn },
      );
    }
    if (props.ssfSigningKey) {
      runtimeEnvironmentSecrets.push(
        { name: 'SSF_SIGNING_KEY', value: props.ssfSigningKey.secretArn },
      );
    }

    // Auto-scaling: cap at 2 instances to prevent cost surprises.
    // We deploy from ECR (no build fees), but a traffic spike or DDoS
    // could scale to 25 instances by default (~$125/hr). This limits exposure.
    const autoScalingConfig = new apprunner.CfnAutoScalingConfiguration(this, 'AutoScaling', {
      // Omit name — let CloudFormation auto-generate to avoid conflicts
      maxConcurrency: 100,  // requests per instance before scaling
      maxSize: 2,           // max 2 instances — increase when revenue justifies it
      minSize: 1,           // always keep 1 warm
    });

    // App Runner Service
    const service = new apprunner.CfnService(this, 'AppRunner', {
      serviceName: props.serviceName,
      sourceConfiguration: {
        authenticationConfiguration: {
          accessRoleArn: accessRole.roleArn,
        },
        // Immutable release tags are promoted explicitly via deploy workflow/rollback scripts.
        autoDeploymentsEnabled: false,
        imageRepository: {
          imageIdentifier: `${this.repository.repositoryUri}:${props.imageTag}`,
          imageRepositoryType: 'ECR',
          imageConfiguration: {
            port: '3001',
            // No startCommand override — the Rust app constructs DATABASE_URL
            // from individual DB_* env vars to avoid shell quoting issues
            // with special characters in the RDS auto-generated password.
            runtimeEnvironmentVariables: [
              ...(props.kmsKeyring ? [{ name:'FAUTH_REQUIRE_KMS', value:'true' }, { name:'FAUTH_KMS_KEYRING_JSON', value:JSON.stringify(props.kmsKeyring) }] : []),
              { name: 'PORT', value: '3001' },
              { name: 'HOST', value: '0.0.0.0' },
              { name: 'SERVER_URL', value: props.serverUrl },
              { name: 'JWT_ISSUER', value: 'gatewasm' },
              { name: 'JWT_AUDIENCE', value: 'gatewasm-app' },
              { name: 'JWT_ACCESS_TOKEN_EXPIRY', value: '900' },
              { name: 'JWT_REFRESH_TOKEN_EXPIRY', value: '2592000' },
              { name: 'ALLOWED_ORIGINS', value: props.allowedOrigins },
              { name: 'PLATFORM_DOMAIN', value: props.platformDomain },
              { name: 'PLATFORM_OWNER_EMAILS', value: props.platformOwnerEmails },
              { name: 'COMPLIANCE_MODE', value: 'standard' },
              { name: 'MAXMIND_DB_PATH', value: '/opt/maxmind/GeoLite2-City.mmdb' },
              { name: 'RUST_LOG', value: 'info' },
            ],
            runtimeEnvironmentSecrets: [
              ...runtimeEnvironmentSecrets,
            ],
          },
        },
      },
      autoScalingConfigurationArn: autoScalingConfig.attrAutoScalingConfigurationArn,
      instanceConfiguration: {
        cpu: '0.25 vCPU',
        memory: '0.5 GB',
        instanceRoleArn: instanceRole.roleArn,
      },
      healthCheckConfiguration: {
        protocol: 'HTTP',
        path: '/health',
        interval: 10,
        timeout: 5,
        healthyThreshold: 1,
        unhealthyThreshold: 5,
      },
      networkConfiguration: {
        egressConfiguration: {
          egressType: 'VPC',
          vpcConnectorArn: vpcConnector.attrVpcConnectorArn,
        },
      },
    });

    this.serviceUrl = service.attrServiceUrl;
    this.serviceId = service.attrServiceId;
    this.serviceName = props.serviceName;

    new CfnOutput(this, 'ServiceUrl', {
      value: `https://${service.attrServiceUrl}`,
      description: 'App Runner service URL',
    });

    new CfnOutput(this, 'EcrRepoUri', {
      value: this.repository.repositoryUri,
      description: 'ECR repository URI for docker push',
    });
  }
}
