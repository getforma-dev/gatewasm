import * as cdk from 'aws-cdk-lib';
import * as ec2 from 'aws-cdk-lib/aws-ec2';
import * as rds from 'aws-cdk-lib/aws-rds';
import * as secretsmanager from 'aws-cdk-lib/aws-secretsmanager';
import { Construct } from 'constructs';
import { Service, KmsKeyring } from '../constructs/service';
import { Monitoring } from '../constructs/monitoring';
import { OwnerSessionCanary } from '../constructs/owner-session-canary';

export interface ServiceStackProps extends cdk.StackProps {
  vpc: ec2.Vpc;
  vpcConnectorSecurityGroup: ec2.SecurityGroup;
  rdsInstance: rds.DatabaseInstance;
  imageTag: string;
  kmsKeyring?: KmsKeyring;
  repositoryName: string;
  serviceName: string;
  domainName: string;
  authSubdomain: string;
  platformOwnerEmails: string;
  environmentName: string;
  alertsEmail?: string;
  ownerSessionSecretArn?: string;
  ownerSessionCanaryScheduleMinutes?: number;
  /** Map of secret name → full ARN for pre-existing Secrets Manager secrets */
  secretArns: Record<string, string>;
}

export class ServiceStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: ServiceStackProps) {
    super(scope, id, props);

    if (props.kmsKeyring && (props.secretArns.Ed25519Key || props.secretArns.SsfSigningKey)) {
      throw new Error('KMS custody forbids process-readable signing secret references');
    }

    const authDomain = `${props.authSubdomain}.${props.domainName}`;

    // Import pre-existing Secrets Manager secrets by full ARN.
    // These are created outside CDK (see RUNBOOK.md) to avoid the
    // chicken-and-egg problem: App Runner needs valid values at startup.
    // Full ARNs (not partial) are required for App Runner runtimeEnvironmentSecrets.
    const secret = (name: string) =>
      secretsmanager.Secret.fromSecretCompleteArn(this, name, props.secretArns[name]);
    const optionalSecret = (name: string) => {
      const arn = props.secretArns[name];
      if (!arn) {
        return undefined;
      }
      return secretsmanager.Secret.fromSecretCompleteArn(this, `${name}Optional`, arn);
    };
    const smtpSecret = optionalSecret('Smtp');
    const mfaEncryptionKeySecret = optionalSecret('MfaEncryptionKey');
    const ssfSigningKeySecret = optionalSecret('SsfSigningKey');

    const service = new Service(this, 'Service', {
      vpc: props.vpc,
      vpcConnectorSecurityGroup: props.vpcConnectorSecurityGroup,
      rdsInstance: props.rdsInstance,
      imageTag: props.imageTag,
      repositoryName: props.repositoryName,
      serviceName: props.serviceName,
      ed25519Key: props.kmsKeyring ? undefined : secret('Ed25519Key'),
      kmsKeyring: props.kmsKeyring,
      csrfSecret: secret('CsrfSecret'),
      platformAdminKey: secret('PlatformAdminKey'),
      oauthGoogle: secret('OAuthGoogle'),
      oauthGithub: secret('OAuthGithub'),
      smtp: smtpSecret,
      mfaEncryptionKey: mfaEncryptionKeySecret,
      ssfSigningKey: props.kmsKeyring ? undefined : ssfSigningKeySecret,
      serverUrl: `https://${authDomain}`,
      platformDomain: props.domainName,
      platformOwnerEmails: props.platformOwnerEmails,
      allowedOrigins: `https://${authDomain},https://${props.domainName}`,
    });

    const monitoring = new Monitoring(this, 'Monitoring', {
      serviceName: service.serviceName,
      serviceId: service.serviceId,
      alarmEmail: props.alertsEmail,
      environmentName: props.environmentName,
    });

    if (props.ownerSessionSecretArn && props.ownerSessionSecretArn.trim().length > 0) {
      new OwnerSessionCanary(this, 'OwnerSessionCanary', {
        environmentName: props.environmentName,
        baseUrl: `https://${authDomain}`,
        ownerSessionSecretArn: props.ownerSessionSecretArn,
        alertsTopic: monitoring.alertsTopic,
        scheduleMinutes: props.ownerSessionCanaryScheduleMinutes,
      });
    }
  }
}
