const assert = require('node:assert/strict');
const test = require('node:test');
const cdk = require('aws-cdk-lib');
const { Template } = require('aws-cdk-lib/assertions');
const ec2 = require('aws-cdk-lib/aws-ec2');
const rds = require('aws-cdk-lib/aws-rds');
const secretsmanager = require('aws-cdk-lib/aws-secretsmanager');
const { Service } = require('../dist/lib/constructs/service');
const { ServiceStack } = require('../dist/lib/stacks/service-stack');

const env = { account: '111122223333', region: 'us-east-1' };
const keyArn = (name) => `arn:aws:kms:us-east-1:111122223333:key/${name}`;
const secretArn = (name) => `arn:aws:secretsmanager:us-east-1:111122223333:secret:${name}-ABCDEF`;
const privateSecretArns = [secretArn('private-access-key'), secretArn('private-ssf-key')];
const keyring = {
  version: 8,
  max_access_lifetime: 900,
  publication: { next_key_id: keyArn('next'), all_replicas_serving_since: 1793145600, evidence: 'synthetic-publication-receipt' },
  active: keyArn('active'),
  next: keyArn('next'),
  ssf: keyArn('ssf'),
  previous: [
    { key_id: keyArn('previous-one'), last_issued_at: 1793145600, retire_after: 1793146500, drain_receipt: 'synthetic-drain-receipt' },
    { key_id: keyArn('previous-two'), last_issued_at: null, retire_after: null, drain_receipt: null },
  ],
};
const secretArns = {
  CsrfSecret: secretArn('csrf'),
  PlatformAdminKey: secretArn('platform-admin'),
  OAuthGoogle: secretArn('google'),
  OAuthGithub: secretArn('github'),
  Smtp: secretArn('smtp'),
  MfaEncryptionKey: secretArn('mfa'),
};

// All resources are synthesized locally with synthetic identifiers. No lookups,
// credentials, deployed resources, or AWS requests are involved.
function infrastructure() {
  const app = new cdk.App();
  const stack = new cdk.Stack(app, 'Infrastructure', { env });
  const vpc = new ec2.Vpc(stack, 'Vpc', {
    maxAzs: 2,
    natGateways: 0,
    subnetConfiguration: [{ name: 'Private', subnetType: ec2.SubnetType.PRIVATE_WITH_EGRESS }],
  });
  const vpcConnectorSecurityGroup = new ec2.SecurityGroup(stack, 'ConnectorSecurityGroup', { vpc });
  const rdsInstance = new rds.DatabaseInstance(stack, 'Database', {
    vpc,
    engine: rds.DatabaseInstanceEngine.postgres({ version: rds.PostgresEngineVersion.VER_16_4 }),
    credentials: rds.Credentials.fromGeneratedSecret('synthetic_user'),
  });
  return { app, stack, vpc, vpcConnectorSecurityGroup, rdsInstance };
}

function serviceFixture(overrides = {}) {
  const base = infrastructure();
  const imported = (name, arn = secretArn(name)) => secretsmanager.Secret.fromSecretCompleteArn(base.stack, name, arn);
  const props = {
    ...base,
    imageTag: 'test-immutable-revision',
    repositoryName: 'synthetic-repository',
    serviceName: 'synthetic-auth',
    kmsKeyring: structuredClone(keyring),
    csrfSecret: imported('csrf'),
    platformAdminKey: imported('platform-admin'),
    oauthGoogle: imported('google'),
    oauthGithub: imported('github'),
    smtp: imported('smtp'),
    mfaEncryptionKey: imported('mfa'),
    serverUrl: 'https://auth.example.test',
    platformDomain: 'example.test',
    platformOwnerEmails: '',
    allowedOrigins: 'https://example.test',
    ...overrides,
  };
  if (overrides.ed25519Key === true) props.ed25519Key = imported('PrivateAccess', privateSecretArns[0]);
  if (overrides.ssfSigningKey === true) props.ssfSigningKey = imported('PrivateSsf', privateSecretArns[1]);
  new Service(base.stack, 'Service', props);
  return Template.fromStack(base.stack).toJSON();
}

function stackFixture(overrides = {}) {
  const base = infrastructure();
  const stack = new ServiceStack(base.app, 'Auth', {
    ...base,
    env,
    imageTag: 'test-immutable-revision',
    repositoryName: 'synthetic-repository',
    serviceName: 'synthetic-auth',
    domainName: 'example.test',
    authSubdomain: 'auth',
    platformOwnerEmails: '',
    environmentName: 'test',
    kmsKeyring: structuredClone(keyring),
    secretArns: { ...secretArns },
    ...overrides,
  });
  return Template.fromStack(stack).toJSON();
}

const array = (value) => Array.isArray(value) ? value : [value];
const sorted = (values) => [...values].sort();
function serviceAndPolicies(template) {
  const services = Object.values(template.Resources).filter((resource) => resource.Type === 'AWS::AppRunner::Service');
  assert.equal(services.length, 1);
  const service = services[0].Properties;
  const roleId = service.InstanceConfiguration.InstanceRoleArn['Fn::GetAtt'][0];
  const policies = Object.values(template.Resources).filter((resource) => resource.Type === 'AWS::IAM::Policy'
    && resource.Properties.Roles.some((role) => role.Ref === roleId));
  const statements = policies.flatMap((policy) => array(policy.Properties.PolicyDocument.Statement));
  assert.ok(statements.length > 0, 'inspect the App Runner instance role, not an unrelated role');
  return { service, statements };
}

function assertKmsBoundary(template) {
  const { service, statements } = serviceAndPolicies(template);
  const allowedActions = new Set(['kms:Sign', 'kms:GetPublicKey', 'kms:DescribeKey', 'secretsmanager:GetSecretValue', 'secretsmanager:DescribeSecret']);
  for (const statement of statements) {
    assert.equal(statement.NotAction, undefined, 'negative action grants are not part of the instance role');
    assert.ok(array(statement.Action).every((action) => allowedActions.has(action)), 'no wildcard or unrelated instance-role permissions');
  }
  const kmsStatements = statements.filter((statement) => array(statement.Action).some((action) => action.startsWith('kms:')));
  assert.equal(kmsStatements.length, 2, 'only public metadata and restricted signing grants are allowed');
  const sign = kmsStatements.find((statement) => array(statement.Action).includes('kms:Sign'));
  assert.deepEqual(array(sign.Action), ['kms:Sign']);
  assert.equal(sign.Effect, 'Allow');
  assert.deepEqual(sorted(array(sign.Resource)), sorted([keyring.active, keyring.ssf]));
  assert.deepEqual(sign.Condition, { StringEquals: { 'kms:SigningAlgorithm': 'ED25519_SHA_512', 'kms:MessageType': 'RAW' } });
  const metadata = kmsStatements.find((statement) => array(statement.Action).includes('kms:GetPublicKey'));
  assert.deepEqual(sorted(array(metadata.Action)), ['kms:DescribeKey', 'kms:GetPublicKey']);
  assert.equal(metadata.Effect, 'Allow');
  assert.deepEqual(sorted(array(metadata.Resource)), sorted([keyring.active, keyring.next, keyring.ssf, ...keyring.previous.map((key) => key.key_id)]));
  assert.equal(metadata.Condition, undefined);

  const configuration = service.SourceConfiguration.ImageRepository.ImageConfiguration;
  const variables = Object.fromEntries(configuration.RuntimeEnvironmentVariables.map(({ Name, Value }) => [Name, Value]));
  const secrets = Object.fromEntries(configuration.RuntimeEnvironmentSecrets.map(({ Name, Value }) => [Name, Value]));
  assert.equal(variables.FAUTH_REQUIRE_KMS, 'true');
  assert.deepEqual(JSON.parse(variables.FAUTH_KMS_KEYRING_JSON), keyring);
  for (const name of ['ED25519_PRIVATE_KEY', 'SSF_SIGNING_KEY']) {
    assert.equal(variables[name], undefined, `${name} must not be in ordinary environment variables`);
    assert.equal(secrets[name], undefined, `${name} must not be in secret environment variables`);
  }
  for (const arn of privateSecretArns) {
    assert.ok(!JSON.stringify(statements).includes(arn), 'no grant to read a private signing secret');
    assert.ok(!JSON.stringify(configuration).includes(arn), 'no private signing secret reference reaches App Runner');
  }
  for (const name of ['CSRF_SECRET', 'PLATFORM_ADMIN_KEY', 'MFA_ENCRYPTION_KEY', 'DB_PASSWORD']) {
    assert.ok(secrets[name], `ordinary application secret ${name} remains available`);
  }
  const readSecrets = statements.filter((statement) => array(statement.Action).includes('secretsmanager:GetSecretValue'));
  assert.ok(readSecrets.length > 0);
  const readResources = readSecrets.flatMap((statement) => array(statement.Resource));
  assert.equal(readResources.length, Object.keys(secretArns).length + 1, 'only ordinary app secrets and the generated RDS secret are readable');
  assert.deepEqual(sorted(readResources.filter((resource) => typeof resource === 'string')), sorted(Object.values(secretArns)));
}

test('Service grants only active and SSF signing, exact public key reads, and no private signing secrets', () => {
  assertKmsBoundary(serviceFixture());
});

test('ServiceStack preserves the same custody boundary in the synthesized deployment', () => {
  assertKmsBoundary(stackFixture());
});

for (const name of ['ed25519Key', 'ssfSigningKey']) {
  test(`Service rejects KMS combined with ${name}`, () => {
    assert.throws(() => serviceFixture({ [name]: true }), /KMS custody forbids process-readable signing secrets/);
  });
}

for (const [name, arn] of [['Ed25519Key', privateSecretArns[0]], ['SsfSigningKey', privateSecretArns[1]]]) {
  test(`ServiceStack rejects a KMS keyring combined with ${name} configuration`, () => {
    assert.throws(() => stackFixture({ secretArns: { ...secretArns, [name]: arn } }), /KMS custody forbids process-readable signing secret references/);
  });
}

test('development signing remains explicit and has no KMS permission or managed-mode flag', () => {
  const { service, statements } = serviceAndPolicies(serviceFixture({ kmsKeyring: undefined, ed25519Key: true, ssfSigningKey: true }));
  assert.ok(!statements.some((statement) => array(statement.Action).some((action) => action.startsWith('kms:'))));
  const configuration = service.SourceConfiguration.ImageRepository.ImageConfiguration;
  const secrets = Object.fromEntries(configuration.RuntimeEnvironmentSecrets.map(({ Name, Value }) => [Name, Value]));
  assert.equal(secrets.ED25519_PRIVATE_KEY, privateSecretArns[0]);
  assert.equal(secrets.SSF_SIGNING_KEY, privateSecretArns[1]);
  assert.ok(!configuration.RuntimeEnvironmentVariables.some(({ Name }) => Name === 'FAUTH_REQUIRE_KMS' || Name === 'FAUTH_KMS_KEYRING_JSON'));
});

test('missing signing configuration is rejected', () => {
  assert.throws(() => serviceFixture({ kmsKeyring: undefined }), /Signing configuration is required/);
});

for (const [name, mutate] of [
  ['duplicate next key', (ring) => { ring.next = ring.active; }],
  ['duplicate previous key', (ring) => { ring.previous[1].key_id = ring.ssf; }],
  ['mutable alias ARN', (ring) => { ring.active = 'arn:aws:kms:us-east-1:111122223333:alias/active'; }],
  ['wildcard key ARN', (ring) => { ring.active = keyArn('*'); }],
  ['bare key identifier', (ring) => { ring.active = 'active'; }],
]) {
  test(`Service rejects ${name}`, () => {
    const ring = structuredClone(keyring);
    mutate(ring);
    assert.throws(() => serviceFixture({ kmsKeyring: ring }), /distinct immutable key ARNs/);
  });
}
