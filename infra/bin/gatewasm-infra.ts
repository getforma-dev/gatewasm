#!/usr/bin/env node
import * as cdk from 'aws-cdk-lib';
import { NetworkStack } from '../lib/stacks/network-stack';
import { DatabaseStack } from '../lib/stacks/database-stack';
import { KmsKeyring } from '../lib/constructs/service';
import { ServiceStack } from '../lib/stacks/service-stack';
import { CdnStack } from '../lib/stacks/cdn-stack';

const app = new cdk.App();

interface EnvironmentConfig {
  stackPrefix: string;
  environmentName: string;
  domainName: string;
  authSubdomain: string;
  platformOwnerEmails: string;
  imageTag: string;
  kmsKeyring?: KmsKeyring;
  serviceName: string;
  repositoryName: string;
  databaseName: string;
  credentialsSecretName: string;
  secretArns: Record<string, string>;
  alertsEmail?: string;
  ownerSessionSecretArn?: string;
  ownerSessionCanaryScheduleMinutes?: number;
  applyLegacySecretLogicalIdOverride: boolean;
}

function requiredContext<T>(key: string): T {
  const value = app.node.tryGetContext(key) as T | undefined;
  if (value === undefined || value === null) {
    throw new Error(`Missing required CDK context: ${key}`);
  }
  return value;
}

function readProdConfig(): EnvironmentConfig {
  const imageTag = app.node.tryGetContext('gatewasm:imageTag') as string | undefined;
  if (!imageTag) {
    throw new Error('Missing required CDK context: gatewasm:imageTag');
  }

  return {
    stackPrefix: 'Gatewasm',
    environmentName: 'prod',
    domainName: requiredContext<string>('gatewasm:domainName'),
    authSubdomain: requiredContext<string>('gatewasm:authSubdomain'),
    platformOwnerEmails:
      (app.node.tryGetContext('gatewasm:platformOwnerEmails') as string | undefined) ??
      'victor@getforma.dev',
    imageTag,
    kmsKeyring: app.node.tryGetContext('gatewasm:kmsKeyring') as KmsKeyring | undefined,
    serviceName: (app.node.tryGetContext('gatewasm:serviceName') as string | undefined) ?? 'gatewasm-server',
    repositoryName:
      (app.node.tryGetContext('gatewasm:repositoryName') as string | undefined) ?? 'gatewasm-server',
    databaseName: (app.node.tryGetContext('gatewasm:databaseName') as string | undefined) ?? 'gatewasm',
    credentialsSecretName:
      (app.node.tryGetContext('gatewasm:rdsSecretName') as string | undefined) ?? 'gatewasm/rds-credentials',
    secretArns: requiredContext<Record<string, string>>('gatewasm:secretArns'),
    alertsEmail: app.node.tryGetContext('gatewasm:alertsEmail') as string | undefined,
    ownerSessionSecretArn: app.node.tryGetContext('gatewasm:ownerSessionSecretArn') as
      | string
      | undefined,
    ownerSessionCanaryScheduleMinutes: app.node.tryGetContext(
      'gatewasm:ownerSessionCanaryScheduleMinutes',
    ) as number | undefined,
    applyLegacySecretLogicalIdOverride: true,
  };
}

function readStagingConfig(baseImageTag: string): EnvironmentConfig | null {
  const enabled = Boolean(app.node.tryGetContext('gatewasm:staging:enabled'));
  if (!enabled) {
    return null;
  }

  const secretArns = app.node.tryGetContext('gatewasm:staging:secretArns') as
    | Record<string, string>
    | undefined;
  if (!secretArns) {
    throw new Error('Missing required CDK context: gatewasm:staging:secretArns');
  }

  return {
    stackPrefix: 'GatewasmStaging',
    environmentName: 'staging',
    domainName:
      (app.node.tryGetContext('gatewasm:staging:domainName') as string | undefined) ??
      requiredContext<string>('gatewasm:domainName'),
    authSubdomain:
      (app.node.tryGetContext('gatewasm:staging:authSubdomain') as string | undefined) ??
      'auth-staging',
    platformOwnerEmails:
      (app.node.tryGetContext('gatewasm:staging:platformOwnerEmails') as string | undefined) ??
      ((app.node.tryGetContext('gatewasm:platformOwnerEmails') as string | undefined) ??
        'victor@getforma.dev'),
    kmsKeyring: app.node.tryGetContext('gatewasm:staging:kmsKeyring') as KmsKeyring | undefined,
    imageTag:
      (app.node.tryGetContext('gatewasm:staging:imageTag') as string | undefined) ?? baseImageTag,
    serviceName:
      (app.node.tryGetContext('gatewasm:staging:serviceName') as string | undefined) ??
      'gatewasm-server-staging',
    repositoryName:
      (app.node.tryGetContext('gatewasm:staging:repositoryName') as string | undefined) ??
      ((app.node.tryGetContext('gatewasm:repositoryName') as string | undefined) ?? 'gatewasm-server'),
    databaseName:
      (app.node.tryGetContext('gatewasm:staging:databaseName') as string | undefined) ??
      'gatewasm_staging',
    credentialsSecretName:
      (app.node.tryGetContext('gatewasm:staging:rdsSecretName') as string | undefined) ??
      'gatewasm-staging/rds-credentials',
    secretArns,
    alertsEmail: app.node.tryGetContext('gatewasm:staging:alertsEmail') as string | undefined,
    ownerSessionSecretArn: app.node.tryGetContext('gatewasm:staging:ownerSessionSecretArn') as
      | string
      | undefined,
    ownerSessionCanaryScheduleMinutes: app.node.tryGetContext(
      'gatewasm:staging:ownerSessionCanaryScheduleMinutes',
    ) as number | undefined,
    applyLegacySecretLogicalIdOverride: false,
  };
}

// ── Configuration from CDK context (cdk.json) ────────────────────────
const env: cdk.Environment = {
  account: process.env.CDK_DEFAULT_ACCOUNT ?? app.node.tryGetContext('aws:cdk:account'),
  region: process.env.CDK_DEFAULT_REGION ?? 'us-east-1',
};
const prodConfig = readProdConfig();
const stagingConfig = readStagingConfig(prodConfig.imageTag);

function createEnvironmentStacks(config: EnvironmentConfig): void {
  // ── Multi-stack architecture ────────────────────────────────────────
  // Deploy order: Network → Database → Service (CDK resolves via addDependency)
  const networkStack = new NetworkStack(app, `${config.stackPrefix}NetworkStack`, { env });

  const databaseStack = new DatabaseStack(app, `${config.stackPrefix}DatabaseStack`, {
    env,
    vpc: networkStack.vpc,
    rdsSecurityGroup: networkStack.rdsSecurityGroup,
    databaseName: config.databaseName,
    credentialsSecretName: config.credentialsSecretName,
    applyLegacySecretLogicalIdOverride: config.applyLegacySecretLogicalIdOverride,
  });
  databaseStack.addDependency(networkStack);

  const serviceStack = new ServiceStack(app, `${config.stackPrefix}ServiceStack`, {
    env,
    vpc: networkStack.vpc,
    vpcConnectorSecurityGroup: networkStack.vpcConnectorSecurityGroup,
    rdsInstance: databaseStack.instance,
    imageTag: config.imageTag,
    kmsKeyring: config.kmsKeyring,
    repositoryName: config.repositoryName,
    serviceName: config.serviceName,
    domainName: config.domainName,
    authSubdomain: config.authSubdomain,
    platformOwnerEmails: config.platformOwnerEmails,
    environmentName: config.environmentName,
    alertsEmail: config.alertsEmail,
    ownerSessionSecretArn: config.ownerSessionSecretArn,
    ownerSessionCanaryScheduleMinutes: config.ownerSessionCanaryScheduleMinutes,
    secretArns: config.secretArns,
  });
  serviceStack.addDependency(networkStack);
  serviceStack.addDependency(databaseStack);
}

createEnvironmentStacks(prodConfig);
if (stagingConfig) {
  createEnvironmentStacks(stagingConfig);
}

// ── CDN (independent — no dependency on other stacks) ────────────
const cdnSubdomain: string = app.node.getContext('gatewasm:cdnSubdomain') ?? 'cdn';
const cdnCertificateArn: string = app.node.getContext('gatewasm:cdnCertificateArn');

new CdnStack(app, 'GatewasmCdnStack', {
  env,
  cdnDomain: `${cdnSubdomain}.${prodConfig.domainName}`,
  certificateArn: cdnCertificateArn,
});
