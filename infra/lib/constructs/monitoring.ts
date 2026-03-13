import { Construct } from 'constructs';
import { CfnOutput, Duration } from 'aws-cdk-lib';
import * as cloudwatch from 'aws-cdk-lib/aws-cloudwatch';
import * as cloudwatchActions from 'aws-cdk-lib/aws-cloudwatch-actions';
import * as logs from 'aws-cdk-lib/aws-logs';
import * as sns from 'aws-cdk-lib/aws-sns';
import * as subscriptions from 'aws-cdk-lib/aws-sns-subscriptions';

export interface MonitoringProps {
  serviceName: string;
  serviceId: string;
  alarmEmail?: string;
  environmentName: string;
}

export class Monitoring extends Construct {
  public readonly alertsTopic: sns.Topic;

  constructor(scope: Construct, id: string, props: MonitoringProps) {
    super(scope, id);

    const applicationLogGroup = logs.LogGroup.fromLogGroupName(
      this,
      'AppRunnerApplicationLogs',
      `/aws/apprunner/${props.serviceName}/${props.serviceId}/application`,
    );

    this.alertsTopic = new sns.Topic(this, 'AuthAlertsTopic', {
      displayName: `GateWASM ${props.environmentName} auth alerts`,
      topicName: `gatewasm-${props.environmentName}-auth-alerts`,
    });

    if (props.alarmEmail && props.alarmEmail.trim().length > 0) {
      this.alertsTopic.addSubscription(
        new subscriptions.EmailSubscription(props.alarmEmail.trim()),
      );
    }

    const missingCookieFilter = new logs.MetricFilter(this, 'MissingCookieFilter', {
      logGroup: applicationLogGroup,
      filterPattern: logs.FilterPattern.literal('"missing_cookie"'),
      metricNamespace: `GateWASM/${props.environmentName}/Auth`,
      metricName: 'OwnerMissingCookieCount',
      metricValue: '1',
      defaultValue: 0,
    });

    const mfaFailureFilter = new logs.MetricFilter(this, 'MfaFailureFilter', {
      logGroup: applicationLogGroup,
      filterPattern: logs.FilterPattern.anyTerm('invalid_mfa_code', 'mfa_locked'),
      metricNamespace: `GateWASM/${props.environmentName}/Auth`,
      metricName: 'MfaFailureCount',
      metricValue: '1',
      defaultValue: 0,
    });

    const tokenErrorFilter = new logs.MetricFilter(this, 'TokenErrorFilter', {
      logGroup: applicationLogGroup,
      filterPattern: logs.FilterPattern.anyTerm(
        'invalid_grant',
        'token_reuse_detected',
        'Refresh token rejected',
      ),
      metricNamespace: `GateWASM/${props.environmentName}/Auth`,
      metricName: 'TokenErrorCount',
      metricValue: '1',
      defaultValue: 0,
    });

    const alarms: cloudwatch.Alarm[] = [
      new cloudwatch.Alarm(this, 'OwnerMissingCookieAlarm', {
        alarmName: `gatewasm-${props.environmentName}-owner-missing-cookie`,
        alarmDescription:
          'Owner dashboard requests are failing due to missing refresh cookies.',
        metric: missingCookieFilter.metric({
          statistic: 'sum',
          period: Duration.minutes(5),
        }),
        threshold: 3,
        evaluationPeriods: 1,
        datapointsToAlarm: 1,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      new cloudwatch.Alarm(this, 'MfaFailureAlarm', {
        alarmName: `gatewasm-${props.environmentName}-mfa-failures`,
        alarmDescription: 'MFA validation is failing repeatedly.',
        metric: mfaFailureFilter.metric({
          statistic: 'sum',
          period: Duration.minutes(5),
        }),
        threshold: 5,
        evaluationPeriods: 1,
        datapointsToAlarm: 1,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
      new cloudwatch.Alarm(this, 'TokenErrorAlarm', {
        alarmName: `gatewasm-${props.environmentName}-token-errors`,
        alarmDescription: 'Token exchange failures exceeded threshold.',
        metric: tokenErrorFilter.metric({
          statistic: 'sum',
          period: Duration.minutes(5),
        }),
        threshold: 5,
        evaluationPeriods: 1,
        datapointsToAlarm: 1,
        treatMissingData: cloudwatch.TreatMissingData.NOT_BREACHING,
      }),
    ];

    for (const alarm of alarms) {
      alarm.addAlarmAction(new cloudwatchActions.SnsAction(this.alertsTopic));
      alarm.addOkAction(new cloudwatchActions.SnsAction(this.alertsTopic));
    }

    new CfnOutput(this, 'AuthAlertsTopicArn', {
      value: this.alertsTopic.topicArn,
      description: `SNS topic for ${props.environmentName} auth alerts`,
    });
  }
}

