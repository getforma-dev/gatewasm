import * as cdk from 'aws-cdk-lib';
import { Construct } from 'constructs';
import { Cdn } from '../constructs/cdn';

export interface CdnStackProps extends cdk.StackProps {
  cdnDomain: string;
  certificateArn: string;
}

export class CdnStack extends cdk.Stack {
  constructor(scope: Construct, id: string, props: CdnStackProps) {
    super(scope, id, props);

    new Cdn(this, 'Cdn', {
      cdnDomain: props.cdnDomain,
      certificateArn: props.certificateArn,
    });
  }
}
