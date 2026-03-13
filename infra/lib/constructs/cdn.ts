import { Construct } from 'constructs';
import * as s3 from 'aws-cdk-lib/aws-s3';
import * as cloudfront from 'aws-cdk-lib/aws-cloudfront';
import * as origins from 'aws-cdk-lib/aws-cloudfront-origins';
import * as acm from 'aws-cdk-lib/aws-certificatemanager';
import * as s3deploy from 'aws-cdk-lib/aws-s3-deployment';
import { CfnOutput, Duration, RemovalPolicy } from 'aws-cdk-lib';

export interface CdnProps {
  /** Full domain for CDN, e.g. "cdn.getforma.dev" */
  cdnDomain: string;
  /** ARN of a pre-validated ACM certificate for cdnDomain */
  certificateArn: string;
}

export class Cdn extends Construct {
  public readonly distribution: cloudfront.Distribution;
  public readonly bucket: s3.Bucket;

  constructor(scope: Construct, id: string, props: CdnProps) {
    super(scope, id);

    // S3 bucket for static assets (bridge.js, future SDK assets)
    this.bucket = new s3.Bucket(this, 'Assets', {
      blockPublicAccess: s3.BlockPublicAccess.BLOCK_ALL,
      encryption: s3.BucketEncryption.S3_MANAGED,
      removalPolicy: RemovalPolicy.RETAIN,
      versioned: true,
    });

    const certificate = acm.Certificate.fromCertificateArn(
      this, 'Cert', props.certificateArn,
    );

    // CloudFront distribution with OAC → S3
    this.distribution = new cloudfront.Distribution(this, 'Distribution', {
      defaultBehavior: {
        origin: origins.S3BucketOrigin.withOriginAccessControl(this.bucket),
        viewerProtocolPolicy: cloudfront.ViewerProtocolPolicy.REDIRECT_TO_HTTPS,
        cachePolicy: new cloudfront.CachePolicy(this, 'CachePolicy', {
          defaultTtl: Duration.hours(24),
          maxTtl: Duration.days(365),
          minTtl: Duration.minutes(0),
          enableAcceptEncodingGzip: true,
          enableAcceptEncodingBrotli: true,
          headerBehavior: cloudfront.CacheHeaderBehavior.none(),
          queryStringBehavior: cloudfront.CacheQueryStringBehavior.none(),
        }),
        responseHeadersPolicy: new cloudfront.ResponseHeadersPolicy(this, 'CorsHeaders', {
          corsBehavior: {
            accessControlAllowOrigins: ['*'],
            accessControlAllowHeaders: ['*'],
            accessControlAllowMethods: ['GET', 'HEAD', 'OPTIONS'],
            accessControlAllowCredentials: false,
            accessControlMaxAge: Duration.days(1),
            originOverride: true,
          },
          customHeadersBehavior: {
            customHeaders: [
              {
                header: 'Cross-Origin-Resource-Policy',
                value: 'cross-origin',
                override: true,
              },
            ],
          },
        }),
      },
      domainNames: [props.cdnDomain],
      certificate,
      priceClass: cloudfront.PriceClass.PRICE_CLASS_100, // US + EU only
      httpVersion: cloudfront.HttpVersion.HTTP2_AND_3,
    });

    // Deploy bridge.js files to S3
    new s3deploy.BucketDeployment(this, 'DeployBridge', {
      sources: [s3deploy.Source.asset('../bridge')],
      destinationBucket: this.bucket,
      destinationKeyPrefix: 'bridge/v1',
      distribution: this.distribution,
      distributionPaths: ['/bridge/*'],
    });

    new CfnOutput(this, 'CdnUrl', {
      value: `https://${props.cdnDomain}`,
      description: 'CDN base URL',
    });
    new CfnOutput(this, 'BridgeJsUrl', {
      value: `https://${props.cdnDomain}/bridge/v1/gate-bridge.min.js`,
      description: 'CDN URL for bridge.js',
    });
    new CfnOutput(this, 'DistributionDomain', {
      value: this.distribution.distributionDomainName,
      description: 'CloudFront distribution domain (for CNAME at registrar)',
    });
    new CfnOutput(this, 'DistributionId', {
      value: this.distribution.distributionId,
      description: 'CloudFront distribution ID for cache invalidation',
    });
  }
}
