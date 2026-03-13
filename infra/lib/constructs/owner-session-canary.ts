import { Construct } from 'constructs';
import { CfnOutput, Duration } from 'aws-cdk-lib';
import * as cloudwatch from 'aws-cdk-lib/aws-cloudwatch';
import * as cloudwatchActions from 'aws-cdk-lib/aws-cloudwatch-actions';
import * as iam from 'aws-cdk-lib/aws-iam';
import * as secretsmanager from 'aws-cdk-lib/aws-secretsmanager';
import * as sns from 'aws-cdk-lib/aws-sns';
import * as synthetics from 'aws-cdk-lib/aws-synthetics';

export interface OwnerSessionCanaryProps {
  environmentName: string;
  baseUrl: string;
  ownerSessionSecretArn: string;
  alertsTopic?: sns.ITopic;
  scheduleMinutes?: number;
}

export class OwnerSessionCanary extends Construct {
  public readonly canary: synthetics.Canary;

  constructor(scope: Construct, id: string, props: OwnerSessionCanaryProps) {
    super(scope, id);

    const scheduleMinutes = props.scheduleMinutes ?? 5;

    this.canary = new synthetics.Canary(this, 'OwnerSessionCanary', {
      canaryName: `gatewasm-${props.environmentName}-owner-session`,
      runtime: synthetics.Runtime.SYNTHETICS_NODEJS_PUPPETEER_9_1,
      schedule: synthetics.Schedule.rate(Duration.minutes(scheduleMinutes)),
      test: synthetics.Test.custom({
        code: synthetics.Code.fromInline(`
const synthetics = require('Synthetics');
const log = require('SyntheticsLogger');
const AWS = require('aws-sdk');

async function readSecret(secretId) {
  const sm = new AWS.SecretsManager();
  const result = await sm.getSecretValue({ SecretId: secretId }).promise();
  const raw = result.SecretString || '{}';
  return JSON.parse(raw);
}

async function writeSecret(secretId, value) {
  const sm = new AWS.SecretsManager();
  await sm.putSecretValue({
    SecretId: secretId,
    SecretString: JSON.stringify(value),
  }).promise();
}

async function postJson(url, body) {
  const response = await fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  let parsed = {};
  try {
    parsed = text ? JSON.parse(text) : {};
  } catch (err) {
    throw new Error('Failed to parse JSON from ' + url + ': ' + err.message + ' body=' + text.slice(0, 500));
  }
  if (!response.ok) {
    throw new Error('Request failed ' + url + ' status=' + response.status + ' body=' + text.slice(0, 500));
  }
  return parsed;
}

exports.handler = async () => {
  const page = await synthetics.getPage();
  const baseUrl = (process.env.BASE_URL || '').replace(/\\/$/, '');
  const secretArn = process.env.OWNER_SESSION_SECRET_ARN;

  if (!baseUrl) throw new Error('BASE_URL env var is required');
  if (!secretArn) throw new Error('OWNER_SESSION_SECRET_ARN env var is required');

  const secret = await readSecret(secretArn);
  const refreshToken = secret.owner_refresh_token;
  if (!refreshToken) {
    throw new Error('owner_refresh_token missing in owner session secret');
  }

  const token = await postJson(baseUrl + '/t/platform/auth/token', {
    grant_type: 'refresh_token',
    refresh_token: refreshToken,
  });

  if (!token.refresh_token) {
    throw new Error('Token response missing refresh_token');
  }

  secret.owner_refresh_token = token.refresh_token;
  secret.last_checked_at = new Date().toISOString();
  await writeSecret(secretArn, secret);

  await page.context().addCookies([
    {
      name: 'gw_rt_platform',
      value: token.refresh_token,
      url: baseUrl,
      path: '/',
      httpOnly: true,
      secure: true,
      sameSite: 'None',
    },
  ]);

  const response = await page.goto(baseUrl + '/platform/admin/dashboard', {
    waitUntil: 'networkidle',
    timeout: 45000,
  });

  if (!response) {
    throw new Error('No response from owner dashboard');
  }
  if (response.status() !== 200) {
    throw new Error('Owner dashboard returned status ' + response.status());
  }
  if (page.url().includes('/platform/login')) {
    throw new Error('Owner dashboard redirected to login');
  }

  log.info('Owner session canary passed: dashboard reachable with rotated owner session');
};
`),
        handler: 'index.handler',
      }),
      environmentVariables: {
        BASE_URL: props.baseUrl,
        OWNER_SESSION_SECRET_ARN: props.ownerSessionSecretArn,
      },
      timeout: Duration.minutes(2),
      successRetentionPeriod: Duration.days(14),
      failureRetentionPeriod: Duration.days(14),
      startAfterCreation: true,
    });

    this.canary.role?.addToPrincipalPolicy(
      new iam.PolicyStatement({
        actions: ['secretsmanager:GetSecretValue', 'secretsmanager:PutSecretValue'],
        resources: [props.ownerSessionSecretArn],
      }),
    );

    const secret = secretsmanager.Secret.fromSecretCompleteArn(
      this,
      'OwnerSessionSecret',
      props.ownerSessionSecretArn,
    );
    secret.grantRead(this.canary.role!);
    secret.grantWrite(this.canary.role!);

    const failedAlarm = new cloudwatch.Alarm(this, 'OwnerSessionCanaryFailed', {
      alarmName: `gatewasm-${props.environmentName}-owner-session-canary-failed`,
      alarmDescription: 'Owner session synthetic check failed.',
      metric: this.canary.metricFailed({
        period: Duration.minutes(scheduleMinutes),
        statistic: 'sum',
      }),
      threshold: 1,
      evaluationPeriods: 1,
      datapointsToAlarm: 1,
      treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
    });

    if (props.alertsTopic) {
      failedAlarm.addAlarmAction(new cloudwatchActions.SnsAction(props.alertsTopic));
      failedAlarm.addOkAction(new cloudwatchActions.SnsAction(props.alertsTopic));
    }

    new CfnOutput(this, 'OwnerSessionCanaryName', {
      value: this.canary.canaryName,
      description: `${props.environmentName} owner session canary name`,
    });
  }
}

